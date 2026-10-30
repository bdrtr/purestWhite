//! The pages of the battery measurement (rust/firmware/DESIGN.md, gate 1): full-frame images on
//! the card, `purestwhite-pages/001.bmp` on, upright 480 by 800, in any depth the C++ firmware's
//! image viewer reads (`bembeyaz-bmp`). A page is read into a frame a few rows at a time, yielding
//! to the other loops between them; a pixel is black when its luminance is under half.

use aligned::{A4, Aligned};
use bembeyaz_bmp::{Bmp, Source};
use embassy_futures::yield_now;
use purestwhite_fat::{BlockDevice, File, Volume};
use purestwhite_render::{FRAME_BYTES, HEIGHT, WIDTH, set_black};

/// The most pages counted.
pub const MAX_PAGES: u16 = 999;
/// The bytes of a file the headers are read from: the file header, a 40-byte DIB header and a
/// palette of 256 entries.
const HEADER: usize = 14 + 40 + 256 * 4;
/// The bytes read from the card at a time: whole rows, at most a sector.
const CHUNK: usize = 4096;
/// A pixel darker than this is black.
const DARK: u8 = 128;
/// `WIDTH` and `HEIGHT` as indexes.
#[allow(clippy::cast_sign_loss, reason = "the screen's size")]
const ROW: usize = WIDTH as usize;
#[allow(clippy::cast_sign_loss, reason = "the screen's size")]
const ROWS: usize = HEIGHT as usize;

/// Why a page could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageError<E> {
    File(purestwhite_fat::Error<E>),
    Bmp(bembeyaz_bmp::Error),
    /// Not upright 480 by 800, or rows wider than `CHUNK`.
    Size,
    /// The file ends before its pixels do.
    Truncated,
}

/// The path of page `n` (from 1, at most 999): `purestwhite-pages/001.bmp`.
pub struct PagePath([u8; 25]);

impl PagePath {
    #[must_use]
    pub fn new(n: u16) -> Self {
        let mut path = *b"purestwhite-pages/000.bmp";
        for (at, unit) in [(18, 100), (19, 10), (20, 1)] {
            if let (Some(digit), Ok(value)) = (path.get_mut(at), u8::try_from(n / unit % 10)) {
                *digit = b'0' + value;
            }
        }
        PagePath(path)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.0).unwrap_or_default()
    }
}

/// How many pages there are: from 001 on, until one is missing.
pub async fn count_pages<D: BlockDevice>(volume: &mut Volume<D>) -> u16 {
    let mut count = 0;
    while count < MAX_PAGES && volume.find(PagePath::new(count + 1).as_str()).await.is_ok() {
        count += 1;
    }
    count
}

/// Reads page `n` (from 1) into `frame`: white, then its dark pixels black.
///
/// # Errors
/// The file cannot be read, or it is not a BMP of the screen's size; `frame` is then left as
/// far as it was drawn.
pub async fn load_page<D: BlockDevice>(
    volume: &mut Volume<D>,
    n: u16,
    frame: &mut [u8; FRAME_BYTES],
) -> Result<(), PageError<D::Error>> {
    let entry = volume
        .find(PagePath::new(n).as_str())
        .await
        .map_err(PageError::File)?;
    let mut file = volume.open(&entry).map_err(PageError::File)?;
    let bmp = header(volume, &mut file).await?;
    let row_bytes = usize::try_from(bmp.row_bytes).unwrap_or(usize::MAX);
    if bmp.width != WIDTH || bmp.height != HEIGHT || row_bytes > CHUNK {
        return Err(PageError::Size);
    }
    file.seek(bmp.data_offset);
    frame.fill(0xFF);
    rows(volume, &mut file, &bmp, row_bytes, frame).await
}

/// The BMP's headers, from the start of `file`.
async fn header<D: BlockDevice>(
    volume: &mut Volume<D>,
    file: &mut File,
) -> Result<Bmp, PageError<D::Error>> {
    let mut start = [0u8; HEADER];
    let len = read_full(volume, file, &mut start).await?;
    let mut source = Start {
        bytes: start.get(..len).unwrap_or_default(),
        at: 0,
        size: file.size(),
    };
    let mut bmp = Bmp::default();
    bembeyaz_bmp::parse(&mut source, &mut bmp).map_err(PageError::Bmp)?;
    Ok(bmp)
}

/// The pixel rows, read `CHUNK` bytes at most at a time, drawn into `frame` (upright: the last
/// row of a bottom-up file is the top one).
async fn rows<D: BlockDevice>(
    volume: &mut Volume<D>,
    file: &mut File,
    bmp: &Bmp,
    row_bytes: usize,
    frame: &mut [u8; FRAME_BYTES],
) -> Result<(), PageError<D::Error>> {
    let mut buf: Aligned<A4, [u8; CHUNK]> = Aligned([0; CHUNK]);
    let per_read = CHUNK / row_bytes.max(1);
    let mut lum = [0u8; ROW];
    let mut row = 0;
    while row < ROWS {
        let count = per_read.min(ROWS - row);
        let piece = buf.get_mut(..count * row_bytes).unwrap_or_default();
        if read_full(volume, file, piece).await? < piece.len() {
            return Err(PageError::Truncated);
        }
        for (bytes, file_row) in piece.chunks_exact(row_bytes).zip(row..) {
            let y = if bmp.top_down {
                file_row
            } else {
                ROWS - 1 - file_row
            };
            // False only for a depth `parse` refuses.
            if !bmp.row_luminance(bytes, 0, &mut lum) {
                return Err(PageError::Bmp(bembeyaz_bmp::Error::UnsupportedBpp));
            }
            draw_row(frame, y, &lum);
        }
        row += count;
        yield_now().await;
    }
    Ok(())
}

/// Reads `file` into `buf` until it is full or the file ends; how many bytes it read.
async fn read_full<D: BlockDevice>(
    volume: &mut Volume<D>,
    file: &mut File,
    buf: &mut [u8],
) -> Result<usize, PageError<D::Error>> {
    let mut filled = 0;
    while let Some(rest) = buf.get_mut(filled..)
        && !rest.is_empty()
    {
        match volume.read(file, rest).await.map_err(PageError::File)? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

/// The dark pixels of the upright row `y`, black.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "within the screen"
)]
fn draw_row(frame: &mut [u8; FRAME_BYTES], y: usize, lum: &[u8; ROW]) {
    for (x, _) in lum.iter().enumerate().filter(|&(_, &l)| l < DARK) {
        set_black(frame, x as i32, y as i32);
    }
}

/// The file's first bytes, as `bembeyaz_bmp::parse` reads them: past them it reads 0s, and a seek
/// within the file succeeds.
struct Start<'a> {
    bytes: &'a [u8],
    at: usize,
    size: u32,
}

impl Source for Start<'_> {
    fn read(&mut self, buf: &mut [u8]) -> usize {
        let rest = self.bytes.get(self.at..).unwrap_or_default();
        let n = rest.len().min(buf.len());
        if let (Some(to), Some(from)) = (buf.get_mut(..n), rest.get(..n)) {
            to.copy_from_slice(from);
        }
        self.at += n;
        n
    }

    fn skip(&mut self, count: u32) {
        self.at = self.at.saturating_add(count as usize);
    }

    fn seek(&mut self, position: u32) -> bool {
        self.at = position as usize;
        position <= self.size
    }
}
