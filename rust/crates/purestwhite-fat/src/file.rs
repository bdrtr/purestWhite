//! Files: reading and seeking along a cluster chain.

use crate::{BLOCK, BLOCK_BYTES, BlockDevice, Entry, Error, Volume};

/// An open file: its chain, its size, and where the next read starts.
#[derive(Debug, PartialEq, Eq)]
pub struct File {
    first: u32,
    size: u32,
    pos: u32,
    /// The cluster holding `pos` once a read found it, and its place in the chain.
    cluster: u32,
    ordinal: u32,
}

impl File {
    #[must_use]
    pub fn size(&self) -> u32 {
        self.size
    }

    #[must_use]
    pub fn position(&self) -> u32 {
        self.pos
    }

    /// Moves the next read to `pos`, at most the end.
    pub fn seek(&mut self, pos: u32) {
        self.pos = pos.min(self.size);
    }
}

impl<D: BlockDevice> Volume<D> {
    /// Opens the file `entry` for reading from its start.
    ///
    /// # Errors
    /// When `entry` is a directory, its first cluster is not one of the volume's, or it is bigger
    /// than the volume (so a chain walked for it is never longer than the volume's clusters).
    pub fn open(&self, entry: &Entry) -> Result<File, Error<D::Error>> {
        if entry.is_dir() {
            return Err(Error::NotAFile);
        }
        let room = u64::from(self.layout.cluster_count) * u64::from(self.layout.cluster_bytes());
        if entry.size > 0
            && (!self.layout.is_cluster(entry.cluster) || u64::from(entry.size) > room)
        {
            return Err(Error::Corrupt);
        }
        Ok(File {
            first: entry.cluster,
            size: entry.size,
            pos: 0,
            cluster: 0,
            ordinal: 0,
        })
    }

    /// Points `file` at the `ordinal`th cluster of its chain: on from where it is, or from the
    /// start when that is behind.
    async fn locate(&mut self, file: &mut File, ordinal: u32) -> Result<(), Error<D::Error>> {
        if file.cluster == 0 || file.ordinal > ordinal {
            file.cluster = file.first;
            file.ordinal = 0;
        }
        while file.ordinal < ordinal {
            file.cluster = self
                .next_cluster(file.cluster)
                .await?
                .ok_or(Error::Corrupt)?;
            file.ordinal += 1;
        }
        Ok(())
    }

    /// Reads from `file`'s position into `buf`, as much as fits and the file holds; the bytes
    /// read, 0 at the end. Whole blocks of clusters that follow each other on the volume come in
    /// one device call, straight into `buf`.
    ///
    /// # Errors
    /// When the device fails or the chain ends before the file does.
    pub async fn read(
        &mut self,
        file: &mut File,
        buf: &mut [u8],
    ) -> Result<usize, Error<D::Error>> {
        let cluster_bytes = self.layout.cluster_bytes();
        let cluster_blocks = self.layout.cluster_blocks;
        // A buffer past 4 GiB reads no more than a file can hold.
        let room = u32::try_from(buf.len()).unwrap_or(u32::MAX);
        let mut done = 0u32;
        while done < room && file.pos < file.size {
            let left = (room - done).min(file.size - file.pos);
            let ordinal = file.pos / cluster_bytes;
            self.locate(file, ordinal).await?;
            let offset = file.pos % cluster_bytes;
            let block = self.layout.cluster_block(file.cluster) + offset / BLOCK_BYTES;
            let in_block = offset % BLOCK_BYTES;
            let out = buf
                .get_mut(done as usize..(done + left) as usize)
                .ok_or(Error::Corrupt)?;
            let taken = if in_block == 0 && left >= BLOCK_BYTES {
                // Whole blocks: to the end of this cluster and on through the clusters right
                // after it, as long as the chain goes there.
                let wanted = left / BLOCK_BYTES;
                let mut run = cluster_blocks - offset / BLOCK_BYTES;
                let mut last = file.cluster;
                while run < wanted {
                    match self.next_cluster(last).await? {
                        Some(next) if next == last + 1 => {
                            last = next;
                            run += cluster_blocks;
                        }
                        _ => break,
                    }
                }
                let bytes = run.min(wanted) * BLOCK_BYTES;
                let (chunks, _) = out
                    .get_mut(..bytes as usize)
                    .ok_or(Error::Corrupt)?
                    .as_chunks_mut::<BLOCK>();
                self.read_blocks(block, chunks).await?;
                // The read ended in the run's clusters, which follow each other.
                let end_ordinal = (file.pos + bytes - 1) / cluster_bytes;
                file.cluster += end_ordinal - ordinal;
                file.ordinal = end_ordinal;
                bytes
            } else {
                let data = self.data_block(block).await?;
                let n = left.min(BLOCK_BYTES - in_block);
                out.get_mut(..n as usize)
                    .zip(data.get(in_block as usize..(in_block + n) as usize))
                    .map(|(to, from)| to.copy_from_slice(from))
                    .ok_or(Error::Corrupt)?;
                n
            };
            done += taken;
            file.pos += taken;
        }
        Ok(done as usize)
    }
}
