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

mod boot;
mod bytes;
mod dir;
mod file;
mod short;
#[cfg(feature = "testing")]
pub mod testing;
#[cfg(test)]
mod tests;
mod upcase;
mod write;
#[cfg(test)]
mod write_tests;

pub(crate) use boot::{Layout, is_fsinfo};
use boot::{fsinfo_next, layout, partition};
pub(crate) use bytes::{byte_at, u16_at, u32_at};
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
