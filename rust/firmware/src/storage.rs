//! Files on the SD card loaded whole into memory, for data read often and changed never: a font
//! that is then drawn without reading the card again. An `async fn` that awaits only the card, so
//! an Embassy task awaits it and an image without an executor drives it with `crate::ready` over
//! the blocking bus.

use allocator_api2::alloc::Allocator;
use allocator_api2::vec::Vec;
use purestwhite_fat::{BlockDevice, Volume};

/// Why a file could not be loaded.
#[derive(Clone, Copy, Debug)]
pub enum LoadError<E> {
    /// The file is not on the volume, or cannot be opened.
    File(purestwhite_fat::Error<E>),
    /// The allocator has no room for it.
    Memory,
    /// It could not be read whole.
    Read,
}

/// Reads the file at `path` on `volume` whole into memory from `alloc`: PSRAM, for a large file
/// (`esp_alloc::ExternalMemory`, or a heap of its own). The card's DMA fills the volume's own
/// internal buffer; the bytes are copied from there, so `alloc`'s memory need not suit DMA.
///
/// # Errors
///
/// The file is not there, `alloc` has no room for it, or it cannot be read whole.
pub async fn load_whole<D: BlockDevice, A: Allocator>(
    volume: &mut Volume<D>,
    path: &str,
    alloc: A,
) -> Result<Vec<u8, A>, LoadError<D::Error>> {
    let entry = volume.find(path).await.map_err(LoadError::File)?;
    let mut file = volume.open(&entry).map_err(LoadError::File)?;
    let size = file.size() as usize;
    let mut bytes = Vec::new_in(alloc);
    bytes
        .try_reserve_exact(size)
        .map_err(|_| LoadError::Memory)?;
    bytes.resize(size, 0);
    let mut at = 0;
    while at < size {
        let rest = bytes.get_mut(at..).unwrap_or_default();
        match volume.read(&mut file, rest).await {
            Ok(n) if n > 0 => at += n,
            _ => return Err(LoadError::Read),
        }
    }
    Ok(bytes)
}
