//! A part of a flash as a flash of its own: offsets from the part's start, and nothing outside it
//! reachable. The firmware's `write-check` uses one on the last megabyte of its own slot to prove
//! the update gate's write path.

use embedded_storage::nor_flash::{
    ErrorType, NorFlash, NorFlashError, NorFlashErrorKind, ReadNorFlash,
};

#[cfg(test)]
mod tests;

/// Why a window refused an access, or what the flash under it failed with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowError<E> {
    /// Past the window's end.
    OutOfBounds,
    Flash(E),
}

impl<E: NorFlashError> NorFlashError for WindowError<E> {
    fn kind(&self) -> NorFlashErrorKind {
        match self {
            WindowError::OutOfBounds => NorFlashErrorKind::OutOfBounds,
            WindowError::Flash(e) => e.kind(),
        }
    }
}

/// `len` bytes of `flash` from `base`.
pub struct Window<'f, F> {
    flash: &'f mut F,
    base: u32,
    len: u32,
}

impl<'f, F: NorFlash> Window<'f, F> {
    /// The window, when it lies within `flash` and starts and ends on a multiple of its erase size.
    pub fn new(flash: &'f mut F, base: u32, len: u32) -> Option<Self> {
        let erase = u32::try_from(F::ERASE_SIZE).ok()?;
        let end = base.checked_add(len)?;
        let fits = usize::try_from(end).is_ok_and(|end| end <= flash.capacity());
        (fits && base.is_multiple_of(erase) && len.is_multiple_of(erase)).then_some(Window {
            flash,
            base,
            len,
        })
    }

    /// `offset` in the flash, when `bytes` from it lie within the window.
    fn at(&self, offset: u32, bytes: usize) -> Result<u32, WindowError<F::Error>> {
        let bytes = u32::try_from(bytes).map_err(|_| WindowError::OutOfBounds)?;
        let end = offset.checked_add(bytes).ok_or(WindowError::OutOfBounds)?;
        if end > self.len {
            return Err(WindowError::OutOfBounds);
        }
        Ok(self.base + offset)
    }
}

impl<F: NorFlash> ErrorType for Window<'_, F> {
    type Error = WindowError<F::Error>;
}

impl<F: NorFlash> ReadNorFlash for Window<'_, F> {
    const READ_SIZE: usize = F::READ_SIZE;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        let at = self.at(offset, bytes.len())?;
        self.flash.read(at, bytes).map_err(WindowError::Flash)
    }

    fn capacity(&self) -> usize {
        self.len as usize
    }
}

impl<F: NorFlash> NorFlash for Window<'_, F> {
    const WRITE_SIZE: usize = F::WRITE_SIZE;
    const ERASE_SIZE: usize = F::ERASE_SIZE;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        let len = to.checked_sub(from).ok_or(WindowError::OutOfBounds)?;
        let at = self.at(from, len as usize)?;
        self.flash.erase(at, at + len).map_err(WindowError::Flash)
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        let at = self.at(offset, bytes.len())?;
        self.flash.write(at, bytes).map_err(WindowError::Flash)
    }
}
