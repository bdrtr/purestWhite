//! Writing, on a device that can ([`BlockWrite`]): clusters taken and given back through every
//! copy of the FAT, directory entries with long names, and files created, written and removed.
//! As in `SdFat`, a file's writes reach its entry (size, first cluster) when it is synced or
//! closed. FAT32's `FSInfo` free count is not kept either; unlike `SdFat`, which leaves it stale,
//! the first FAT change of a mount marks it unknown.
//!
//! The order of the writes keeps a write cut off between two blocks from doing harm beyond what
//! `fsck.fat` mends: a file's data, then the FAT, then its entry; a new cluster is marked as a
//! chain's end before the chain points at it; a directory's new cluster is zeroed before it joins
//! the directory; long-name pieces go before their short entry; a removed file's entries are
//! marked deleted before its clusters are freed. So a cut leaves at most lost clusters, a file
//! shorter than its chain, or a long name without its entry, and no entry points at a free
//! cluster, as long as no open file is removed (see [`File`]). (An SD card cut off while writing may also damage blocks it wrote shortly before,
//! which no order guards against.)

use crate::dir::{
    ATTR_DIRECTORY, ATTR_LONG_NAME, DELETED, ENTRIES_PER_BLOCK, ENTRY, MAX_ENTRIES, PIECE_OFFSETS,
    PIECE_UNITS, checksum, same_name, short_matches, trim_name,
};
use crate::short::{basis, valid_long};
use crate::{
    BLOCK, BLOCK_BYTES, Block, BlockWrite, Dir, Entry, Error, File, Kind, Name, Volume, is_fsinfo,
    u32_at,
};

/// A file's attribute: changed since the last backup, which every new or written file gets.
const ATTR_ARCHIVE: u8 = 0x20;
/// The numeric tails one pass over a directory looks at: a window of 1024. A directory of at
/// most 65 536 entries leaves one of 1 to 65 537 free, so 65 passes at most find it.
const TAIL_WINDOW: u32 = 1024;
const TAIL_WINDOWS: u32 = 65;

/// A date and time for directory entries, kept as FAT keeps them: local time from 1980 to 2107,
/// to two seconds. Out-of-range fields are clamped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timestamp {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl Default for Timestamp {
    /// 1980-01-01 00:00:00, FAT's first moment: a device without a clock.
    fn default() -> Self {
        Timestamp {
            year: 1980,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
        }
    }
}

impl Timestamp {
    fn date(self) -> u16 {
        ((self.year.clamp(1980, 2107) - 1980) << 9)
            | (u16::from(self.month.clamp(1, 12)) << 5)
            | u16::from(self.day.clamp(1, 31))
    }

    fn time(self) -> u16 {
        (u16::from(self.hour.min(23)) << 11)
            | (u16::from(self.minute.min(59)) << 5)
            | u16::from(self.second.min(59) / 2)
    }
}

fn put_u16(raw: &mut [u8], at: usize, value: u16) {
    if let Some(to) = raw.get_mut(at..at + 2) {
        to.copy_from_slice(&value.to_le_bytes());
    }
}

fn put_u32(raw: &mut [u8], at: usize, value: u32) {
    if let Some(to) = raw.get_mut(at..at + 4) {
        to.copy_from_slice(&value.to_le_bytes());
    }
}

/// The free entries of a directory as one pass found them.
struct Room {
    /// The first of `need` free entries in a row, if there are.
    run: Option<u32>,
    /// The free entries at the directory's end, and how many entries it has.
    trailing: u32,
    total: u32,
}

impl<D: BlockWrite> Volume<D> {
    /// Sets the time that new and written entries get from now on.
    pub fn set_time(&mut self, now: Timestamp) {
        self.stamp = now;
    }

    /// Writes the FAT block being changed to every copy of the FAT in turn (only to the one in
    /// use when mirroring is off); the first time, `FSInfo`'s free count is marked unknown first.
    async fn flush_fat(&mut self) -> Result<(), Error<D::Error>> {
        let l = self.layout;
        let Some(at) = self.fat_w.at.filter(|_| self.fat_dirty) else {
            return Ok(());
        };
        if !self.fsinfo_marked {
            self.mark_fsinfo().await?;
        }
        let within = at - l.fat_start;
        let copies = if l.mirrored {
            0..l.fats
        } else {
            let active = (l.fat_start - l.fats_start) / l.fat_blocks;
            active..active + 1
        };
        for copy in copies {
            let block = l.fats_start + copy * l.fat_blocks + within;
            self.dev
                .write(block, core::slice::from_ref(&self.fat_w.block))
                .await
                .map_err(Error::Device)?;
        }
        self.fat_dirty = false;
        Ok(())
    }

    /// Sets `cluster`'s FAT entry to `value` (FAT32 keeps the entry's top four bits), in the
    /// block being changed; it reaches the device by `flush_fat`.
    async fn set_fat(&mut self, cluster: u32, value: u32) -> Result<(), Error<D::Error>> {
        let l = self.layout;
        if !l.is_cluster(cluster) {
            return Err(Error::Corrupt);
        }
        let (block, at) = l.fat_place(cluster);
        let block = l.fat_start + block;
        if self.fat_w.at != Some(block) {
            self.flush_fat().await?;
            self.fat_w.at = None;
            let read = self.fat.load(&mut self.dev, block).await?;
            self.fat_w.block = *read;
            self.fat_w.at = Some(block);
        }
        let raw = &mut self.fat_w.block;
        match l.kind {
            Kind::Fat16 => put_u16(raw, at, u16::try_from(value).unwrap_or(u16::MAX)),
            Kind::Fat32 => {
                let top = u32_at(raw, at) & 0xF000_0000;
                put_u32(raw, at, top | (value & 0x0FFF_FFFF));
            }
        }
        self.fat_dirty = true;
        Ok(())
    }

    /// The value that ends a chain.
    fn chain_end(&self) -> u32 {
        match self.layout.kind {
            Kind::Fat16 => 0xFFFF,
            Kind::Fat32 => 0x0FFF_FFFF,
        }
    }

    /// Takes a free cluster and marks it as a chain's end, looking first right after `near` (so
    /// a growing chain stays in one piece) or where the last one was found.
    /// After a look that found none, the volume is full until a cluster is freed.
    async fn allocate(&mut self, near: Option<u32>) -> Result<u32, Error<D::Error>> {
        let l = self.layout;
        if self.full {
            return Err(Error::Full);
        }
        let last = l.cluster_count + 1;
        let start = match near.map(|c| c + 1) {
            Some(c) if l.is_cluster(c) => c,
            _ if l.is_cluster(self.next_free) => self.next_free,
            _ => 2,
        };
        let mut cluster = start;
        while self.fat_entry(cluster).await? != 0 {
            cluster = if cluster == last { 2 } else { cluster + 1 };
            if cluster == start {
                self.full = true;
                return Err(Error::Full);
            }
        }
        self.set_fat(cluster, self.chain_end()).await?;
        self.next_free = if cluster == last { 2 } else { cluster + 1 };
        Ok(cluster)
    }

    /// Frees the chain from `first`. A chain that loops or runs off the volume is an error once
    /// it reaches a cluster already freed or not a cluster.
    async fn free_chain(&mut self, first: u32) -> Result<(), Error<D::Error>> {
        let mut cluster = first;
        loop {
            let next = self.next_cluster(cluster).await?;
            self.set_fat(cluster, 0).await?;
            self.full = false;
            match next {
                Some(next) => cluster = next,
                None => return Ok(()),
            }
        }
    }

    /// Marks `FSInfo`'s free count unknown and writes the next free cluster, if the volume has a
    /// valid `FSInfo`; once a mount.
    async fn mark_fsinfo(&mut self) -> Result<(), Error<D::Error>> {
        let Some(at) = self.layout.fsinfo else {
            self.fsinfo_marked = true;
            return Ok(());
        };
        let mut block = [0u8; BLOCK];
        self.dev
            .read(at, core::slice::from_mut(&mut block))
            .await
            .map_err(Error::Device)?;
        if is_fsinfo(&block) {
            put_u32(&mut block, 488, u32::MAX);
            put_u32(&mut block, 492, self.next_free);
            self.dev
                .write(at, core::slice::from_ref(&block))
                .await
                .map_err(Error::Device)?;
        }
        self.fsinfo_marked = true;
        Ok(())
    }

    /// Writes `bytes` into block `at` from byte `offset`, through the data cache.
    pub(crate) async fn write_part(
        &mut self,
        at: u32,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), Error<D::Error>> {
        self.data.load(&mut self.dev, at).await?;
        let Some(to) = self.data.block.get_mut(offset..offset + bytes.len()) else {
            return Err(Error::Corrupt);
        };
        to.copy_from_slice(bytes);
        let written = self
            .dev
            .write(at, core::slice::from_ref(&self.data.block))
            .await;
        if written.is_err() {
            self.data.at = None;
        }
        written.map_err(Error::Device)
    }

    /// Writes whole blocks from `at` on, past the data cache (which forgets a block among them).
    pub(crate) async fn write_blocks(
        &mut self,
        at: u32,
        blocks: &[Block],
    ) -> Result<(), Error<D::Error>> {
        let count = u32::try_from(blocks.len()).map_err(|_| Error::Corrupt)?;
        if self.data.at.is_some_and(|c| (at..at + count).contains(&c)) {
            self.data.at = None;
        }
        self.dev.write(at, blocks).await.map_err(Error::Device)
    }

    /// Zeroes `cluster`.
    async fn zero_cluster(&mut self, cluster: u32) -> Result<(), Error<D::Error>> {
        let first = self.layout.cluster_block(cluster);
        let zero = [[0u8; BLOCK]; 1];
        for block in first..first + self.layout.cluster_blocks {
            self.write_blocks(block, &zero).await?;
        }
        Ok(())
    }

    /// The block holding entry `index` of `dir`, and the entry's byte in it.
    async fn slot_place(&mut self, dir: Dir, index: u32) -> Result<(u32, usize), Error<D::Error>> {
        let l = self.layout;
        let within = (index % ENTRIES_PER_BLOCK) as usize * ENTRY;
        if dir.cluster == 0 {
            if l.kind != Kind::Fat16 || index >= l.root_entries {
                return Err(Error::Corrupt);
            }
            return Ok((l.root_start + index / ENTRIES_PER_BLOCK, within));
        }
        let per_cluster = l.cluster_blocks * ENTRIES_PER_BLOCK;
        let mut cluster = dir.cluster;
        for _ in 0..index / per_cluster {
            cluster = self.next_cluster(cluster).await?.ok_or(Error::Corrupt)?;
        }
        if !l.is_cluster(cluster) {
            return Err(Error::Corrupt);
        }
        Ok((
            l.cluster_block(cluster) + (index % per_cluster) / ENTRIES_PER_BLOCK,
            within,
        ))
    }

    /// Writes entry `index` of `dir`.
    async fn write_slot(
        &mut self,
        dir: Dir,
        index: u32,
        raw: &[u8; ENTRY],
    ) -> Result<(), Error<D::Error>> {
        let (block, at) = self.slot_place(dir, index).await?;
        self.write_part(block, at, raw).await
    }

    /// Entry `index` of `dir` as it stands.
    pub(crate) async fn read_slot(
        &mut self,
        dir: Dir,
        index: u32,
    ) -> Result<[u8; ENTRY], Error<D::Error>> {
        let (block, at) = self.slot_place(dir, index).await?;
        let data = self.data_block(block).await?;
        data.get(at..at + ENTRY)
            .and_then(|r| r.try_into().ok())
            .ok_or(Error::Corrupt)
    }

    /// `dir`'s first run of `need` free entries, its free entries at the end, and its size in
    /// entries.
    async fn room(&mut self, dir: Dir, need: u32) -> Result<Room, Error<D::Error>> {
        let mut cursor = self.cursor(dir);
        let (mut run_start, mut run_len, mut run) = (0, 0, None);
        let mut ended = false;
        while let Some(raw) = self.raw_entry(&mut cursor).await? {
            let index = cursor.index - 1;
            // Past the end mark every entry is free, whatever it holds.
            ended |= raw[0] == 0;
            if ended || raw[0] == DELETED {
                if run_len == 0 {
                    run_start = index;
                }
                run_len += 1;
                if run_len == need && run.is_none() {
                    run = Some(run_start);
                }
            } else {
                run_len = 0;
            }
        }
        Ok(Room {
            run,
            trailing: run_len,
            total: cursor.index,
        })
    }

    /// The last cluster of the chain from `first`.
    async fn chain_last(&mut self, first: u32) -> Result<u32, Error<D::Error>> {
        let mut cluster = first;
        for _ in 0..self.layout.cluster_count {
            match self.next_cluster(cluster).await? {
                Some(next) => cluster = next,
                None => return Ok(cluster),
            }
        }
        Err(Error::Corrupt)
    }

    /// Finds room for `need` entries in `dir`, growing a cluster directory by zeroed clusters
    /// when it has none: the first entry's index.
    async fn make_room(&mut self, dir: Dir, need: u32) -> Result<u32, Error<D::Error>> {
        let room = self.room(dir, need).await?;
        if let Some(run) = room.run {
            return Ok(run);
        }
        let start = room.total - room.trailing;
        if dir.cluster == 0 || start + need > MAX_ENTRIES {
            return Err(Error::DirFull);
        }
        let per_cluster = self.layout.cluster_blocks * ENTRIES_PER_BLOCK;
        let mut have = room.total;
        let mut last = self.chain_last(dir.cluster).await?;
        while have < start + need {
            let cluster = self.allocate(Some(last)).await?;
            self.zero_cluster(cluster).await?;
            self.set_fat(last, cluster).await?;
            last = cluster;
            have += per_cluster;
        }
        Ok(start)
    }

    /// A short name for `long` that no entry of `dir` has (`Exists` when `long` is already a
    /// name there): the basis, or the basis with the lowest free numeric tail, looked for a
    /// window of tails a pass.
    async fn pick_short(&mut self, dir: Dir, long: &str) -> Result<[u8; 11], Error<D::Error>> {
        let basis = basis(long);
        for window in 0..TAIL_WINDOWS {
            let low = window * TAIL_WINDOW;
            let mut seen = [0u64; (TAIL_WINDOW / 64) as usize];
            let mut cursor = self.cursor(dir);
            let mut name = Name::new();
            while let Some(entry) = self.next(&mut cursor, &mut name).await? {
                if window == 0
                    && (same_name(name.as_str(), long) || short_matches(&entry.short_name, long))
                {
                    return Err(Error::Exists);
                }
                if let Some(n) = basis
                    .tail_of(&entry.short_name)
                    .and_then(|n| n.checked_sub(low))
                    .filter(|&n| n < TAIL_WINDOW)
                    && let Some(word) = seen.get_mut((n / 64) as usize)
                {
                    *word |= 1 << (n % 64);
                }
            }
            // Without a tail the basis is the name in upper case, which an entry of that short
            // name would have matched above.
            if !basis.tail {
                return Ok(basis.name);
            }
            let free = (0..TAIL_WINDOW).find(|&n| {
                low + n > 0
                    && seen
                        .get((n / 64) as usize)
                        .is_some_and(|word| word & (1 << (n % 64)) == 0)
            });
            if let Some(n) = free {
                return Ok(basis.with_tail(low + n));
            }
        }
        Err(Error::DirFull)
    }

    /// Adds an entry named `long` to `dir`: its long-name pieces (unless `long` is its short name
    /// exactly) and its short entry.
    async fn add_entry(
        &mut self,
        dir: Dir,
        long: &str,
        attributes: u8,
        cluster: u32,
    ) -> Result<Entry, Error<D::Error>> {
        let short = self.pick_short(dir, long).await?;
        let exact = basis(long).exact && short == basis(long).name;
        let units = long.encode_utf16().count();
        let pieces = if exact {
            0
        } else {
            u32::try_from(units.div_ceil(PIECE_UNITS)).map_err(|_| Error::BadName)?
        };
        let first = self.make_room(dir, pieces + 1).await?;
        let sum = checksum(&short);
        for piece in (1..=pieces).rev() {
            let mut raw = [0u8; ENTRY];
            raw[0] = u8::try_from(piece).map_err(|_| Error::BadName)?
                | if piece == pieces { 0x40 } else { 0 };
            raw[11] = ATTR_LONG_NAME;
            raw[13] = sum;
            let skip = (piece as usize - 1) * PIECE_UNITS;
            let mut name_units = long.encode_utf16().skip(skip);
            let mut ended = skip >= units;
            for &at in &PIECE_OFFSETS {
                let unit = if ended {
                    0xFFFF
                } else if let Some(unit) = name_units.next() {
                    unit
                } else {
                    ended = true;
                    0
                };
                put_u16(&mut raw, at, unit);
            }
            self.write_slot(dir, first + (pieces - piece), &raw).await?;
        }
        let index = first + pieces;
        let mut raw = [0u8; ENTRY];
        raw[..11].copy_from_slice(&short);
        raw[11] = attributes;
        let (date, time) = (self.stamp.date(), self.stamp.time());
        put_u16(&mut raw, 14, time);
        put_u16(&mut raw, 16, date);
        put_u16(&mut raw, 18, date);
        put_u16(&mut raw, 22, time);
        put_u16(&mut raw, 24, date);
        self.set_cluster(&mut raw, cluster);
        self.write_slot(dir, index, &raw).await?;
        Ok(Entry {
            attributes,
            cluster,
            size: 0,
            short_name: short,
            parent: dir,
            index,
            first,
        })
    }

    /// Puts `cluster` into a short entry: its high half only on FAT32.
    fn set_cluster(&self, raw: &mut [u8; ENTRY], cluster: u32) {
        if self.layout.kind == Kind::Fat32 {
            put_u16(raw, 20, (cluster >> 16) as u16);
        }
        put_u16(raw, 26, (cluster & 0xFFFF) as u16);
    }

    /// The directory and name of `path`'s last part.
    async fn parent_of<'p>(&mut self, path: &'p str) -> Result<(Dir, &'p str), Error<D::Error>> {
        let path = path.trim_end_matches('/');
        let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
        let name = trim_name(name);
        if !valid_long(name) {
            return Err(Error::BadName);
        }
        Ok((self.open_dir(parent).await?, name))
    }

    /// Creates an empty file at `path`, whose directories must be there, and opens it.
    ///
    /// # Errors
    /// When the name is not one FAT can hold, an entry by that name is there, the directory is
    /// full, the volume is full (a directory needs to grow), or the device fails.
    pub async fn create(&mut self, path: &str) -> Result<File, Error<D::Error>> {
        let (dir, name) = self.parent_of(path).await?;
        let entry = self.add_entry(dir, name, ATTR_ARCHIVE, 0).await;
        self.flush_fat().await?;
        self.open(&entry?)
    }

    /// Writes `data` into `file` from its position on, growing it as needed; the bytes written:
    /// all of `data`, or what fits below 4 GiB. As in `SdFat`, its entry learns the new size and
    /// first cluster when the file is synced or closed.
    ///
    /// # Errors
    /// When the volume is full (what was written before stays, and the file's size says how
    /// much), the file's entry is no longer its own (it was removed while open; then nothing is
    /// written), the file's chain is damaged, or the device fails.
    pub async fn write(&mut self, file: &mut File, data: &[u8]) -> Result<usize, Error<D::Error>> {
        if file.first != 0 && !self.layout.is_cluster(file.first) {
            return Err(Error::Corrupt);
        }
        // A file removed since it was last looked at may have lost its clusters to another one:
        // its entry must still be its own.
        if file.removals != self.removals {
            if !self.is_own_entry(file).await? {
                return Err(Error::NotFound);
            }
            file.removals = self.removals;
        }
        let room = u32::MAX - file.pos;
        let total = u32::try_from(data.len()).unwrap_or(u32::MAX).min(room);
        file.dirty = true;
        let done = self.write_data(file, data, total).await?;
        Ok(done as usize)
    }

    /// Puts `file`'s writes on the device: the FAT, then its entry (size, first cluster, time).
    ///
    /// # Errors
    /// When its entry is no longer its own (the file was removed, and the entry perhaps reused,
    /// while it was open) or the device fails.
    pub async fn sync(&mut self, file: &mut File) -> Result<(), Error<D::Error>> {
        self.flush_fat().await?;
        if file.dirty {
            self.update_entry(file).await?;
            file.dirty = false;
        }
        Ok(())
    }

    /// Syncs `file` and closes it.
    ///
    /// # Errors
    /// As `sync`'s.
    pub async fn close(&mut self, mut file: File) -> Result<(), Error<D::Error>> {
        self.sync(&mut file).await
    }

    /// The data part of `write`: as much as it wrote before an error comes back in the file.
    async fn write_data(
        &mut self,
        file: &mut File,
        data: &[u8],
        total: u32,
    ) -> Result<u32, Error<D::Error>> {
        let l = self.layout;
        let cluster_bytes = l.cluster_bytes();
        let mut done = 0u32;
        while done < total {
            if file.first == 0 {
                file.first = self.allocate(None).await?;
                file.cluster = 0;
            }
            let ordinal = file.pos / cluster_bytes;
            self.locate_growing(file, ordinal).await?;
            let offset = file.pos % cluster_bytes;
            let block = l.cluster_block(file.cluster) + offset / BLOCK_BYTES;
            let in_block = offset % BLOCK_BYTES;
            let left = total - done;
            let from = data
                .get(done as usize..(done + left) as usize)
                .ok_or(Error::Corrupt)?;
            let taken = if in_block == 0 && left >= BLOCK_BYTES {
                let blocks = (left / BLOCK_BYTES).min(l.cluster_blocks - offset / BLOCK_BYTES);
                let bytes = blocks * BLOCK_BYTES;
                let (chunks, _) = from
                    .get(..bytes as usize)
                    .ok_or(Error::Corrupt)?
                    .as_chunks::<BLOCK>();
                self.write_blocks(block, chunks).await?;
                bytes
            } else {
                let n = left.min(BLOCK_BYTES - in_block);
                let part = from.get(..n as usize).ok_or(Error::Corrupt)?;
                self.write_part(block, in_block as usize, part).await?;
                n
            };
            done += taken;
            file.pos += taken;
            file.size = file.size.max(file.pos);
        }
        Ok(done)
    }

    /// As `locate`, but a chain that ends before the `ordinal`th cluster grows to it.
    async fn locate_growing(
        &mut self,
        file: &mut File,
        ordinal: u32,
    ) -> Result<(), Error<D::Error>> {
        if file.cluster == 0 || file.ordinal > ordinal {
            file.cluster = file.first;
            file.ordinal = 0;
        }
        while file.ordinal < ordinal {
            file.cluster = if let Some(next) = self.next_cluster(file.cluster).await? {
                next
            } else {
                let next = self.allocate(Some(file.cluster)).await?;
                self.set_fat(file.cluster, next).await?;
                next
            };
            file.ordinal += 1;
        }
        Ok(())
    }

    /// Writes `file`'s size, first cluster and time into its entry, and marks it changed; an
    /// entry with another short name or first cluster than the file had is not its own.
    async fn update_entry(&mut self, file: &mut File) -> Result<(), Error<D::Error>> {
        if !self.is_own_entry(file).await? {
            return Err(Error::NotFound);
        }
        let mut raw = self.read_slot(file.parent, file.index).await?;
        if raw[11] & (ATTR_DIRECTORY | 0x08) != 0 {
            return Err(Error::Corrupt);
        }
        self.set_cluster(&mut raw, file.first);
        put_u32(&mut raw, 28, file.size);
        raw[11] |= ATTR_ARCHIVE;
        let (date, time) = (self.stamp.date(), self.stamp.time());
        put_u16(&mut raw, 18, date);
        put_u16(&mut raw, 22, time);
        put_u16(&mut raw, 24, date);
        self.write_slot(file.parent, file.index, &raw).await?;
        file.entry_first = file.first;
        Ok(())
    }

    /// Whether `file`'s entry still has the short name and first cluster the file knows.
    async fn is_own_entry(&mut self, file: &File) -> Result<bool, Error<D::Error>> {
        let raw = self.read_slot(file.parent, file.index).await?;
        let high = match self.layout.kind {
            Kind::Fat16 => 0,
            Kind::Fat32 => u32::from(u16::from_le_bytes([raw[20], raw[21]])) << 16,
        };
        let cluster = high | u32::from(u16::from_le_bytes([raw[26], raw[27]]));
        Ok(raw[..11] == file.short && cluster == file.entry_first)
    }

    /// Removes the file at `path`: its entries are marked deleted, then its clusters freed. The
    /// file must not be open (see [`File`]).
    ///
    /// # Errors
    /// When nothing is there, it is a directory, its chain is damaged, or the device fails.
    pub async fn remove(&mut self, path: &str) -> Result<(), Error<D::Error>> {
        let entry = self.find(path).await?;
        if entry.is_dir() {
            return Err(Error::NotAFile);
        }
        let removed = self.remove_entry(&entry).await;
        let settled = self.flush_fat().await;
        removed?;
        settled
    }

    /// Marks `entry`'s slots deleted and frees its clusters.
    async fn remove_entry(&mut self, entry: &Entry) -> Result<(), Error<D::Error>> {
        self.removals = self.removals.wrapping_add(1);
        for index in entry.first..=entry.index {
            let mut raw = self.read_slot(entry.parent, index).await?;
            raw[0] = DELETED;
            self.write_slot(entry.parent, index, &raw).await?;
        }
        if entry.cluster != 0 {
            self.free_chain(entry.cluster).await?;
        }
        Ok(())
    }
}
