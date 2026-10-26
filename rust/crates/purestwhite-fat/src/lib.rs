//! FAT16 and FAT32 on a block device, for the bare-metal firmware's SD card: our own, from
//! Microsoft's FAT specification (`fatgen103`), as the owner chose over a third-party crate.
//! It mounts the volume (the first FAT partition of an MBR, or a card formatted without one),
//! lists directories with their long names, finds paths, and reads files, the blocks of
//! contiguous clusters in one device call straight into the caller's buffer. On a device that can
//! write ([`BlockWrite`]) it also creates files with long names, writes, cuts short and removes
//! them, makes and removes directories, and renames and moves entries: every FAT change goes to each copy of the FAT in turn (only to the one in use when FAT32's
//! mirroring is off), and FAT32's free-cluster count (`FSInfo`) is marked unknown, as `SdFat` does
//! not keep it. Only one writer may have a card mounted at a time: the volume keeps its own
//! FAT cache and free-cluster hints, as `SdFat` keeps its own, and two writers would hand out
//! the same clusters.
//!
//! Names are matched as `SdFat` matches them, so both firmwares find the same file: each UTF-16
//! unit in upper case by `SdFat`'s own table (Turkish ı is not I there), the name looked up
//! without leading spaces or trailing dots and spaces; a short name also matches ignoring ASCII
//! case. Long names are UTF-16; a short name's bytes past ASCII are read in code page 437, FAT's
//! default, and its lower-case flags apply to every letter.
//! FAT12 (volumes of under 4085 clusters), exFAT and blocks other than 512 bytes are refused.
//!
//! Nothing here panics or overflows, and a damaged volume is an error, not a hang: the boot
//! sector's numbers are checked against each other and the partition, so every block reached is
//! inside the volume; a cluster out of range, a chain that points back at the same cluster or ends
//! before its file, and a file bigger than the volume are errors. A longer loop in a chain goes
//! unnoticed, but reads stop at the file's size and listings at a directory's most entries.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

mod dir;
mod file;
mod short;
#[cfg(test)]
mod tests;
mod upcase;
mod write;
#[cfg(test)]
mod write_tests;

pub use dir::{Dir, DirCursor, Entry, Name};
pub use file::File;
pub use write::Timestamp;

/// Bytes in a block, the only size supported.
pub const BLOCK: usize = 512;
pub(crate) const BLOCK_BYTES: u32 = 512;
/// A block of the device.
pub type Block = [u8; BLOCK];

/// The card or image the volume lives on, in 512-byte blocks.
#[allow(
    async_fn_in_trait,
    reason = "single-threaded firmware; no Send bound needed"
)]
pub trait BlockDevice {
    type Error: core::fmt::Debug;

    /// Reads `blocks.len()` blocks, from block `start` on.
    ///
    /// # Errors
    /// When the device fails.
    async fn read(&mut self, start: u32, blocks: &mut [Block]) -> Result<(), Self::Error>;
}

impl<T: BlockDevice> BlockDevice for &mut T {
    type Error = T::Error;

    async fn read(&mut self, start: u32, blocks: &mut [Block]) -> Result<(), Self::Error> {
        (**self).read(start, blocks).await
    }
}

/// A device that can also write.
#[allow(
    async_fn_in_trait,
    reason = "single-threaded firmware; no Send bound needed"
)]
pub trait BlockWrite: BlockDevice {
    /// Writes `blocks.len()` blocks, from block `start` on.
    ///
    /// # Errors
    /// When the device fails.
    async fn write(&mut self, start: u32, blocks: &[Block]) -> Result<(), Self::Error>;
}

impl<T: BlockWrite> BlockWrite for &mut T {
    async fn write(&mut self, start: u32, blocks: &[Block]) -> Result<(), Self::Error> {
        (**self).write(start, blocks).await
    }
}

/// Why a call failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error<E> {
    /// The device failed.
    Device(E),
    /// No FAT volume: no boot sector in block 0 or in the first FAT partition.
    NoVolume,
    /// A FAT12 or exFAT volume, or blocks other than 512 bytes.
    Unsupported,
    /// The volume contradicts itself: a boot sector whose numbers do not fit together or its
    /// partition, a cluster out of range, a chain that points back at the same cluster or ends
    /// before the file does, a file bigger than the volume.
    Corrupt,
    /// No entry by that name.
    NotFound,
    /// A path goes through a file.
    NotADirectory,
    /// A directory opened as a file.
    NotAFile,
    /// An entry by that name is already there.
    Exists,
    /// A name a FAT volume cannot hold: empty, "." or "..", longer than 255 UTF-16 units, or with
    /// a control character or one of `\ / : * ? " < > |`.
    BadName,
    /// No free cluster left.
    Full,
    /// A directory to remove still holds entries.
    NotEmpty,
    /// A directory moved into itself or a directory under it.
    IntoItself,
    /// The directory holds no more entries: FAT16's root is full, a directory has 65 536, or no
    /// numeric tail is left for a short name.
    DirFull,
}

/// The two FAT types supported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Fat16,
    Fat32,
}

/// Where the volume's parts are, in device blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Layout {
    pub(crate) kind: Kind,
    /// The FAT in use.
    pub(crate) fat_start: u32,
    pub(crate) fat_blocks: u32,
    /// The first FAT, how many there are, and whether a change goes to all of them (otherwise
    /// only to the one in use: FAT32 with mirroring off).
    pub(crate) fats_start: u32,
    pub(crate) fats: u32,
    pub(crate) mirrored: bool,
    /// FAT32's `FSInfo` block.
    pub(crate) fsinfo: Option<u32>,
    /// The volume's serial number, when the boot sector has one.
    pub(crate) serial: Option<u32>,
    /// FAT16's fixed root directory: its first block and entry count; FAT32's root is a cluster
    /// chain from `root_cluster`.
    pub(crate) root_start: u32,
    pub(crate) root_entries: u32,
    pub(crate) root_cluster: u32,
    /// Cluster 2's first block.
    pub(crate) data_start: u32,
    pub(crate) cluster_blocks: u32,
    /// Clusters 2 to `cluster_count + 1` exist.
    pub(crate) cluster_count: u32,
}

/// A little-endian `u16` at `at`, or 0 past the end.
fn u16_at(bytes: &[u8], at: usize) -> u16 {
    bytes
        .get(at..at + 2)
        .and_then(|b| b.try_into().ok())
        .map_or(0, u16::from_le_bytes)
}

/// A little-endian `u32` at `at`, or 0 past the end.
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    bytes
        .get(at..at + 4)
        .and_then(|b| b.try_into().ok())
        .map_or(0, u32::from_le_bytes)
}

/// The byte at `at`, or 0 past the end.
fn byte_at(bytes: &[u8], at: usize) -> u8 {
    bytes.get(at).copied().unwrap_or(0)
}

/// Whether `block` ends with the 0x55AA signature.
fn signed(block: &Block) -> bool {
    block.get(510..512) == Some(&[0x55, 0xAA][..])
}

/// The first partition-table entry of a FAT16 (small, large, LBA) or FAT32 (CHS, LBA) type: its
/// first block and length. Without one, the first of type 0x07 (exFAT, or NTFS), so that mounting
/// an exFAT card says it is unsupported rather than that there is no volume.
fn partition(mbr: &Block) -> Option<(u32, u32)> {
    if !signed(mbr) {
        return None;
    }
    let first_of = |fat: bool| {
        (0..4).find_map(|i| {
            let entry = mbr.get(0x1BE + 16 * i..0x1CE + 16 * i)?;
            let kind = byte_at(entry, 4);
            let wanted = if fat {
                matches!(kind, 0x04 | 0x06 | 0x0B | 0x0C | 0x0E)
            } else {
                kind == 0x07
            };
            let (start, count) = (u32_at(entry, 8), u32_at(entry, 12));
            (wanted && start > 0 && count > 0).then_some((start, count))
        })
    };
    first_of(true).or_else(|| first_of(false))
}

/// FAT32's `FSInfo` signatures, at 0, 484 and 508.
const FSINFO_LEAD: u32 = 0x4161_5252;
const FSINFO_STRUCT: u32 = 0x6141_7272;
const FSINFO_TRAIL: u32 = 0xAA55_0000;

/// Whether `block` has `FSInfo`'s signatures.
pub(crate) fn is_fsinfo(block: &Block) -> bool {
    u32_at(block, 0) == FSINFO_LEAD
        && u32_at(block, 484) == FSINFO_STRUCT
        && u32_at(block, 508) == FSINFO_TRAIL
}

/// An `FSInfo` block's next free cluster, if it is one.
fn fsinfo_next(block: &Block) -> Option<u32> {
    is_fsinfo(block).then(|| u32_at(block, 492))
}

/// Whether `boot` is an exFAT boot sector (its OEM name).
fn is_exfat(boot: &Block) -> bool {
    boot.get(3..11) == Some(b"EXFAT   ")
}

/// The volume's layout from its boot sector at block `first`, at most `limit` blocks long (its
/// partition's length) when known. Every block it gives is below `first` plus the volume's
/// length, which fits a `u32`, and every cluster's FAT entry lies inside the FAT.
fn layout<E>(boot: &Block, first: u32, limit: Option<u32>) -> Result<Layout, Error<E>> {
    if is_exfat(boot) {
        return Err(Error::Unsupported);
    }
    let looks_boot = matches!(byte_at(boot, 0), 0xEB | 0xE9) && signed(boot);
    let bytes_per_sector = u16_at(boot, 11);
    let cluster_blocks = u32::from(byte_at(boot, 13));
    let reserved = u32::from(u16_at(boot, 14));
    let fats = u32::from(byte_at(boot, 16));
    if !looks_boot || reserved == 0 || fats == 0 || !cluster_blocks.is_power_of_two() {
        return Err(Error::NoVolume);
    }
    if bytes_per_sector != 512 {
        return Err(Error::Unsupported);
    }
    let root_entries = u32::from(u16_at(boot, 17));
    let total = match u16_at(boot, 19) {
        0 => u32_at(boot, 32),
        n => u32::from(n),
    };
    let fat16_blocks = u16_at(boot, 22);
    let fat_blocks = match fat16_blocks {
        0 => u32_at(boot, 36),
        n => u32::from(n),
    };
    if first.checked_add(total).is_none() || limit.is_some_and(|l| total > l) {
        return Err(Error::Corrupt);
    }
    let root_blocks = (root_entries * 32).div_ceil(512);
    let data = reserved
        .checked_add(fats.checked_mul(fat_blocks).ok_or(Error::Corrupt)?)
        .and_then(|n| n.checked_add(root_blocks))
        .ok_or(Error::Corrupt)?;
    if fat_blocks == 0 || data >= total {
        return Err(Error::Corrupt);
    }
    let cluster_count = (total - data) / cluster_blocks;
    // FAT32's boot sector has no 16-bit FAT size; otherwise the cluster count decides
    // (fatgen103, "FAT Type Determination"). Linux reads a small FAT32 so too.
    let kind = if fat16_blocks == 0 {
        Kind::Fat32
    } else {
        match cluster_count {
            0..4085 => return Err(Error::Unsupported),
            4085..65525 => Kind::Fat16,
            _ => Kind::Fat32,
        }
    };
    let width: u64 = match kind {
        Kind::Fat16 => 2,
        Kind::Fat32 => 4,
    };
    // FAT32's entries hold 28 bits, of which 0x0FFFFFF7 on are marks; every cluster's entry must
    // be in the FAT; FAT16's root needs a size.
    let fat_entries = u64::from(fat_blocks) * BLOCK as u64 / width;
    if (kind == Kind::Fat32 && cluster_count > 0x0FFF_FFF5)
        || u64::from(cluster_count) + 2 > fat_entries
        || (kind == Kind::Fat16 && root_entries == 0)
    {
        return Err(Error::Corrupt);
    }
    // FAT32 with mirroring off uses the FAT its flags name; otherwise the first.
    let flags = u16_at(boot, 40);
    let mirrored = !(kind == Kind::Fat32 && flags & 0x80 != 0);
    let active = if mirrored { 0 } else { u32::from(flags & 0x0F) };
    if active >= fats {
        return Err(Error::Corrupt);
    }
    let root_cluster = if kind == Kind::Fat32 {
        u32_at(boot, 44)
    } else {
        0
    };
    // All below `first + total`, which fits: `data` is below `total`.
    // FSInfo is in the reserved blocks, past the boot sector; 0 or 0xFFFF for none.
    let fsinfo = match u32::from(u16_at(boot, 48)) {
        n if kind == Kind::Fat32 && (1..reserved).contains(&n) => Some(first + n),
        _ => None,
    };
    // The extended boot signature 0x29 says the serial number follows it.
    let signature_at = match kind {
        Kind::Fat16 => 38,
        Kind::Fat32 => 66,
    };
    let serial = (byte_at(boot, signature_at) == 0x29).then(|| u32_at(boot, signature_at + 1));
    let layout = Layout {
        kind,
        serial,
        fat_start: first + reserved + active * fat_blocks,
        fat_blocks,
        fats_start: first + reserved,
        fats,
        mirrored,
        fsinfo,
        root_start: first + reserved + fats * fat_blocks,
        root_entries,
        root_cluster,
        data_start: first + data,
        cluster_blocks,
        cluster_count,
    };
    if kind == Kind::Fat32 && !layout.is_cluster(root_cluster) {
        return Err(Error::Corrupt);
    }
    Ok(layout)
}

impl Layout {
    /// Whether `cluster` is a data cluster of the volume.
    pub(crate) fn is_cluster(&self, cluster: u32) -> bool {
        (2..self.cluster_count + 2).contains(&cluster)
    }

    /// `cluster`'s first block.
    pub(crate) fn cluster_block(&self, cluster: u32) -> u32 {
        self.data_start + (cluster - 2) * self.cluster_blocks
    }

    pub(crate) fn cluster_bytes(&self) -> u32 {
        self.cluster_blocks * BLOCK_BYTES
    }

    /// Where `cluster`'s FAT entry is: its block within a FAT, and its byte in that block.
    pub(crate) fn fat_place(&self, cluster: u32) -> (u32, usize) {
        let width = match self.kind {
            Kind::Fat16 => 2,
            Kind::Fat32 => 4,
        };
        let offset = cluster * width;
        (offset / BLOCK_BYTES, (offset % BLOCK_BYTES) as usize)
    }
}

/// A block kept in memory, and which one it is.
struct Cached {
    block: Block,
    at: Option<u32>,
}

impl Cached {
    const fn new() -> Self {
        Cached {
            block: [0; BLOCK],
            at: None,
        }
    }

    async fn load<D: BlockDevice>(
        &mut self,
        dev: &mut D,
        at: u32,
    ) -> Result<&Block, Error<D::Error>> {
        if self.at != Some(at) {
            self.at = None;
            dev.read(at, core::slice::from_mut(&mut self.block))
                .await
                .map_err(Error::Device)?;
            self.at = Some(at);
        }
        Ok(&self.block)
    }
}

/// A mounted FAT volume.
pub struct Volume<D> {
    dev: D,
    layout: Layout,
    /// A block of the FAT, and a block of a directory or a file read in part.
    fat: Cached,
    data: Cached,
    /// The FAT block being changed, newer than the device's copies while `fat_dirty` (`create`,
    /// `sync`, `close` and `remove` write it out before they return). It is read in place of `fat`'s copy of the same block,
    /// which is older; moving on to another block loads that one through `fat`, so the older copy
    /// is gone by then.
    fat_w: Cached,
    fat_dirty: bool,
    /// What the writes need: where to look for a free cluster first, whether a look found none
    /// since the last one was freed, whether `FSInfo`'s free count was marked unknown yet, and the
    /// time new and written entries get.
    pub(crate) next_free: u32,
    pub(crate) full: bool,
    pub(crate) fsinfo_marked: bool,
    /// Files removed this mount, so an open file knows to look at its entry again.
    pub(crate) removals: u32,
    pub(crate) stamp: Timestamp,
}

impl<D: BlockDevice> Volume<D> {
    /// Mounts the first FAT partition of `dev`'s partition table (without one, an exFAT one, to
    /// say it is unsupported), or the whole device when block 0 is a boot sector itself. The
    /// volume must fit its partition.
    ///
    /// # Errors
    /// When the device fails, or there is no volume, or one of a type not supported.
    pub async fn mount(mut dev: D) -> Result<Self, Error<D::Error>> {
        let mut block = [0; BLOCK];
        dev.read(0, core::slice::from_mut(&mut block))
            .await
            .map_err(Error::Device)?;
        let layout = match partition(&block) {
            Some((start, count)) => {
                dev.read(start, core::slice::from_mut(&mut block))
                    .await
                    .map_err(Error::Device)?;
                layout(&block, start, Some(count))?
            }
            None => layout(&block, 0, None)?,
        };
        // FSInfo's next free cluster, where writes look first. Its free count is not kept up
        // by every writer (SdFat leaves it), so it is not trusted.
        let mut next_free = 2;
        if let Some(at) = layout.fsinfo {
            dev.read(at, core::slice::from_mut(&mut block))
                .await
                .map_err(Error::Device)?;
            if let Some(next) = fsinfo_next(&block).filter(|&n| layout.is_cluster(n)) {
                next_free = next;
            }
        }
        Ok(Volume {
            dev,
            layout,
            fat: Cached::new(),
            data: Cached::new(),
            fat_w: Cached::new(),
            fat_dirty: false,
            next_free,
            full: false,
            fsinfo_marked: false,
            removals: 0,
            stamp: Timestamp::default(),
        })
    }

    /// FAT16 or FAT32.
    #[must_use]
    pub fn kind(&self) -> Kind {
        self.layout.kind
    }

    /// Bytes in a cluster.
    #[must_use]
    pub fn cluster_bytes(&self) -> u32 {
        self.layout.cluster_bytes()
    }

    /// The volume's serial number, set when it was formatted, if its boot sector has one.
    #[must_use]
    pub fn serial(&self) -> Option<u32> {
        self.layout.serial
    }

    /// The device, to set it up between reads. The volume keeps a block of the FAT and one of
    /// data: the device must still give the same bytes.
    pub fn device_mut(&mut self) -> &mut D {
        &mut self.dev
    }

    /// The device back.
    pub fn into_inner(self) -> D {
        self.dev
    }

    /// The cluster after `cluster` in its chain, or `None` at the chain's end.
    pub(crate) async fn next_cluster(
        &mut self,
        cluster: u32,
    ) -> Result<Option<u32>, Error<D::Error>> {
        let l = self.layout;
        if !l.is_cluster(cluster) {
            return Err(Error::Corrupt);
        }
        let end = match l.kind {
            Kind::Fat16 => 0xFFF8,
            Kind::Fat32 => 0x0FFF_FFF8,
        };
        let value = self.fat_entry(cluster).await?;
        if value >= end {
            Ok(None)
        } else if l.is_cluster(value) && value != cluster {
            Ok(Some(value))
        } else {
            // Free, reserved, bad or itself in the middle of a chain.
            Err(Error::Corrupt)
        }
    }

    /// `cluster`'s FAT entry as it stands (FAT32's top four bits dropped); `cluster` is below
    /// `cluster_count + 2`, so the entry is inside the FAT.
    pub(crate) async fn fat_entry(&mut self, cluster: u32) -> Result<u32, Error<D::Error>> {
        let l = self.layout;
        let (block, at) = l.fat_place(cluster);
        let block = l.fat_start + block;
        let block = if self.fat_w.at == Some(block) {
            &self.fat_w.block
        } else {
            self.fat.load(&mut self.dev, block).await?
        };
        Ok(match l.kind {
            Kind::Fat16 => u32::from(u16_at(block, at)),
            Kind::Fat32 => u32_at(block, at) & 0x0FFF_FFFF,
        })
    }

    /// Reads one block through the data cache.
    pub(crate) async fn data_block(&mut self, at: u32) -> Result<&Block, Error<D::Error>> {
        self.data.load(&mut self.dev, at).await
    }

    /// Reads whole blocks straight into `blocks`.
    pub(crate) async fn read_blocks(
        &mut self,
        start: u32,
        blocks: &mut [Block],
    ) -> Result<(), Error<D::Error>> {
        self.dev.read(start, blocks).await.map_err(Error::Device)
    }
}
