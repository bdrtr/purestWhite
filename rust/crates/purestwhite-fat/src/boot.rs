//! Where the volume is and how it is laid out: the MBR's partition table, the boot sector's
//! numbers checked against each other and the partition, and FAT32's `FSInfo` block.

use crate::bytes::{byte_at, u16_at, u32_at};
use crate::{BLOCK, BLOCK_BYTES, Block, Error, Kind};

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

/// Whether `block` ends with the 0x55AA signature.
pub(crate) fn signed(block: &Block) -> bool {
    block.get(510..512) == Some(&[0x55, 0xAA][..])
}

/// The first partition-table entry of a FAT16 (small, large, LBA) or FAT32 (CHS, LBA) type: its
/// first block and length. Without one, the first of type 0x07 (exFAT, or NTFS), so that mounting
/// an exFAT card says it is unsupported rather than that there is no volume.
pub(crate) fn partition(mbr: &Block) -> Option<(u32, u32)> {
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
pub(crate) fn fsinfo_next(block: &Block) -> Option<u32> {
    is_fsinfo(block).then(|| u32_at(block, 492))
}

/// Whether `boot` is an exFAT boot sector (its OEM name).
fn is_exfat(boot: &Block) -> bool {
    boot.get(3..11) == Some(b"EXFAT   ")
}

/// The boot sector's numbers `layout` reads (its BIOS parameter block).
struct Params {
    cluster_blocks: u32,
    reserved: u32,
    fats: u32,
    root_entries: u32,
    /// The volume's length in blocks.
    total: u32,
    /// FAT16's FAT size; 0 on FAT32, whose size is `fat_blocks`.
    fat16_blocks: u16,
    fat_blocks: u32,
}

/// The boot sector's numbers, once it is a FAT boot sector of 512-byte blocks.
fn params<E>(boot: &Block) -> Result<Params, Error<E>> {
    if is_exfat(boot) {
        return Err(Error::Unsupported);
    }
    let looks_boot = matches!(byte_at(boot, 0), 0xEB | 0xE9) && signed(boot);
    let fat16_blocks = u16_at(boot, 22);
    let p = Params {
        cluster_blocks: u32::from(byte_at(boot, 13)),
        reserved: u32::from(u16_at(boot, 14)),
        fats: u32::from(byte_at(boot, 16)),
        root_entries: u32::from(u16_at(boot, 17)),
        total: match u16_at(boot, 19) {
            0 => u32_at(boot, 32),
            n => u32::from(n),
        },
        fat16_blocks,
        fat_blocks: match fat16_blocks {
            0 => u32_at(boot, 36),
            n => u32::from(n),
        },
    };
    if !looks_boot || p.reserved == 0 || p.fats == 0 || !p.cluster_blocks.is_power_of_two() {
        return Err(Error::NoVolume);
    }
    if u16_at(boot, 11) != 512 {
        return Err(Error::Unsupported);
    }
    Ok(p)
}

/// Where the data area starts, in blocks from the volume's first, and how many clusters it
/// holds; the volume at block `first` must fit a `u32` and its partition (`limit`).
fn data_area<E>(p: &Params, first: u32, limit: Option<u32>) -> Result<(u32, u32), Error<E>> {
    if first.checked_add(p.total).is_none() || limit.is_some_and(|l| p.total > l) {
        return Err(Error::Corrupt);
    }
    let root_blocks = (p.root_entries * 32).div_ceil(512);
    let data = p
        .reserved
        .checked_add(p.fats.checked_mul(p.fat_blocks).ok_or(Error::Corrupt)?)
        .and_then(|n| n.checked_add(root_blocks))
        .ok_or(Error::Corrupt)?;
    if p.fat_blocks == 0 || data >= p.total {
        return Err(Error::Corrupt);
    }
    Ok((data, (p.total - data) / p.cluster_blocks))
}

/// The FAT type: FAT32's boot sector has no 16-bit FAT size; otherwise the cluster count decides
/// (fatgen103, "FAT Type Determination"). Linux reads a small FAT32 so too. FAT12 is refused.
fn kind<E>(fat16_blocks: u16, cluster_count: u32) -> Result<Kind, Error<E>> {
    if fat16_blocks == 0 {
        return Ok(Kind::Fat32);
    }
    match cluster_count {
        0..4085 => Err(Error::Unsupported),
        4085..65525 => Ok(Kind::Fat16),
        _ => Ok(Kind::Fat32),
    }
}

/// FAT32's entries hold 28 bits, of which 0x0FFFFFF7 on are marks; every cluster's entry must be
/// in the FAT; FAT16's root needs a size.
fn check_fat<E>(kind: Kind, cluster_count: u32, p: &Params) -> Result<(), Error<E>> {
    let width: u64 = match kind {
        Kind::Fat16 => 2,
        Kind::Fat32 => 4,
    };
    let fat_entries = u64::from(p.fat_blocks) * BLOCK as u64 / width;
    if (kind == Kind::Fat32 && cluster_count > 0x0FFF_FFF5)
        || u64::from(cluster_count) + 2 > fat_entries
        || (kind == Kind::Fat16 && p.root_entries == 0)
    {
        return Err(Error::Corrupt);
    }
    Ok(())
}

/// Whether a change goes to every FAT, and the FAT in use: FAT32 with mirroring off uses the FAT
/// its flags name; otherwise the first.
fn active_fat<E>(boot: &Block, kind: Kind, fats: u32) -> Result<(bool, u32), Error<E>> {
    let flags = u16_at(boot, 40);
    let mirrored = !(kind == Kind::Fat32 && flags & 0x80 != 0);
    let active = if mirrored { 0 } else { u32::from(flags & 0x0F) };
    if active >= fats {
        return Err(Error::Corrupt);
    }
    Ok((mirrored, active))
}

/// FAT32's `FSInfo` block, in the reserved blocks past the boot sector (0 or 0xFFFF for none).
fn fsinfo(boot: &Block, kind: Kind, first: u32, reserved: u32) -> Option<u32> {
    match u32::from(u16_at(boot, 48)) {
        n if kind == Kind::Fat32 && (1..reserved).contains(&n) => Some(first + n),
        _ => None,
    }
}

/// The volume's serial number: the extended boot signature 0x29 says it follows.
fn serial(boot: &Block, kind: Kind) -> Option<u32> {
    let signature_at = match kind {
        Kind::Fat16 => 38,
        Kind::Fat32 => 66,
    };
    (byte_at(boot, signature_at) == 0x29).then(|| u32_at(boot, signature_at + 1))
}

/// The volume's layout from its boot sector at block `first`, at most `limit` blocks long (its
/// partition's length) when known. Every block it gives is below `first` plus the volume's
/// length, which fits a `u32`, and every cluster's FAT entry lies inside the FAT.
pub(crate) fn layout<E>(boot: &Block, first: u32, limit: Option<u32>) -> Result<Layout, Error<E>> {
    let p = params(boot)?;
    let (data, cluster_count) = data_area(&p, first, limit)?;
    let kind = kind(p.fat16_blocks, cluster_count)?;
    check_fat(kind, cluster_count, &p)?;
    let (mirrored, active) = active_fat(boot, kind, p.fats)?;
    let root_cluster = if kind == Kind::Fat32 {
        u32_at(boot, 44)
    } else {
        0
    };
    // All below `first + total`, which fits: `data` is below `total`.
    let layout = Layout {
        kind,
        serial: serial(boot, kind),
        fat_start: first + p.reserved + active * p.fat_blocks,
        fat_blocks: p.fat_blocks,
        fats_start: first + p.reserved,
        fats: p.fats,
        mirrored,
        fsinfo: fsinfo(boot, kind, first, p.reserved),
        root_start: first + p.reserved + p.fats * p.fat_blocks,
        root_entries: p.root_entries,
        root_cluster,
        data_start: first + data,
        cluster_blocks: p.cluster_blocks,
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
