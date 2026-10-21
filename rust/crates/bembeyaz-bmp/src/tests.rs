#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]

use super::{Bmp, Error, Source, parse};
use std::vec::Vec;

extern crate std;

/// A file in memory, read in pieces of at most `chunk` bytes.
struct File {
    data: Vec<u8>,
    position: usize,
    chunk: usize,
}

impl Source for File {
    fn read(&mut self, buf: &mut [u8]) -> usize {
        let rest = self.data.get(self.position..).unwrap_or_default();
        let n = rest.len().min(buf.len()).min(self.chunk);
        buf[..n].copy_from_slice(&rest[..n]);
        self.position += n;
        n
    }
    fn skip(&mut self, count: u32) {
        let target = self.position + count as usize;
        if target <= self.data.len() {
            self.position = target;
        }
    }
    fn seek(&mut self, position: u32) -> bool {
        let ok = position as usize <= self.data.len();
        if ok {
            self.position = position as usize;
        }
        ok
    }
}

/// A BMP with `palette` (BGRA) entries and `rows` of pixel data.
fn bmp(
    width: i32,
    height: i32,
    bpp: u16,
    compression: u32,
    colors: u32,
    palette: &[[u8; 4]],
    rows: &[u8],
) -> Vec<u8> {
    let offset = 54 + 4 * palette.len() as u32;
    let mut v = Vec::new();
    v.extend_from_slice(b"BM");
    v.extend_from_slice(&(offset + rows.len() as u32).to_le_bytes());
    v.extend_from_slice(&[0; 4]);
    v.extend_from_slice(&offset.to_le_bytes());
    v.extend_from_slice(&40u32.to_le_bytes());
    v.extend_from_slice(&width.to_le_bytes());
    v.extend_from_slice(&height.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&bpp.to_le_bytes());
    v.extend_from_slice(&compression.to_le_bytes());
    v.extend_from_slice(&[0; 12]);
    v.extend_from_slice(&colors.to_le_bytes());
    v.extend_from_slice(&[0; 4]);
    for entry in palette {
        v.extend_from_slice(entry);
    }
    v.extend_from_slice(rows);
    v
}

fn open(data: Vec<u8>) -> (Result<(), Error>, Bmp, usize) {
    let mut file = File {
        data,
        position: 0,
        chunk: 3,
    };
    let mut out = Bmp::default();
    let result = parse(&mut file, &mut out);
    (result, out, file.position)
}

#[test]
fn reads_a_paletted_image_and_leaves_the_file_at_its_pixels() {
    let palette = [
        [0, 0, 0, 0],
        [255, 255, 255, 0],
        [0, 0, 255, 0],
        [40, 40, 40, 0],
    ];
    let (result, out, position) = open(bmp(5, -2, 4, 0, 4, &palette, &[0x01, 0x23, 0x00, 0x00]));
    assert_eq!(result, Ok(()));
    assert_eq!(
        (out.width, out.height, out.top_down, out.row_bytes),
        (5, 2, true, 4)
    );
    assert_eq!(
        (out.data_offset, out.colors_used, out.bits_per_pixel),
        (70, 4, 4)
    );
    assert_eq!(position, 70);
    // Red is (77 * 255) >> 8 = 76, close to 85; 40 is too far from 0 and 85.
    assert_eq!(out.palette_lum[..5], [0, 255, 76, 40, 4]);
    assert!(!out.native_palette);
    let mut lum = [0; 4];
    assert!(out.row_luminance(&[0x01, 0x23], 0, &mut lum));
    assert_eq!(lum, [0, 255, 76, 40]);
}

#[test]
fn unpacks_every_depth() {
    let mut out = Bmp {
        bits_per_pixel: 1,
        ..Bmp::default()
    };
    out.palette_lum = core::array::from_fn(|i| (i as u8).wrapping_mul(10));
    let mut lum = [0; 4];
    assert!(out.row_luminance(&[0b1010_0000], 0, &mut lum));
    assert_eq!(lum, [10, 0, 10, 0]);
    out.bits_per_pixel = 2;
    assert!(out.row_luminance(&[0b0001_1011, 0b1100_0000], 2, &mut lum));
    assert_eq!(lum, [20, 30, 30, 0]);
    out.bits_per_pixel = 8;
    assert!(out.row_luminance(&[1, 2, 3], 1, &mut lum[..2]));
    assert_eq!(lum[..2], [20, 30]);
    out.bits_per_pixel = 24;
    assert!(out.row_luminance(&[0, 0, 255, 255, 255, 255], 0, &mut lum[..2]));
    assert_eq!(lum[..2], [76, 255]);
    out.bits_per_pixel = 32;
    assert!(out.row_luminance(&[0, 255, 0, 9, 255, 0, 0, 9], 0, &mut lum[..2]));
    assert_eq!(lum[..2], [149, 28]);
    // Bytes past the row read as 0.
    assert!(out.row_luminance(&[], 3, &mut lum[..1]));
    assert_eq!(lum[0], 0);
    out.bits_per_pixel = 16;
    assert!(!out.row_luminance(&[], 0, &mut []));
}

#[test]
fn refuses_what_bitmap_refused() {
    let gray = [[0, 0, 0, 0], [255, 255, 255, 0]];
    let ok = bmp(8, 8, 1, 0, 0, &gray, &[0; 32]);
    assert_eq!(open(ok.clone()).0, Ok(()));
    assert!(open(ok.clone()).1.native_palette);
    let mut not_bmp = ok.clone();
    not_bmp[0] = b'X';
    assert_eq!(open(not_bmp).0, Err(Error::NotBmp));
    let mut small_dib = ok.clone();
    small_dib[14] = 12;
    assert_eq!(open(small_dib).0, Err(Error::DibTooSmall));
    let mut planes = ok.clone();
    planes[26] = 2;
    assert_eq!(open(planes).0, Err(Error::BadPlanes));
    assert_eq!(
        open(bmp(8, 8, 16, 0, 0, &[], &[])).0,
        Err(Error::UnsupportedBpp)
    );
    assert_eq!(
        open(bmp(8, 8, 24, 3, 0, &[], &[])).0,
        Err(Error::UnsupportedCompression)
    );
    assert_eq!(open(bmp(8, 8, 32, 3, 0, &[], &[0; 256])).0, Ok(()));
    assert_eq!(
        open(bmp(8, 8, 8, 0, 257, &[], &[])).0,
        Err(Error::PaletteTooLarge)
    );
    assert_eq!(
        open(bmp(0, 8, 24, 0, 0, &[], &[])).0,
        Err(Error::BadDimensions)
    );
    assert_eq!(
        open(bmp(8, i32::MIN, 24, 0, 0, &[], &[])).0,
        Err(Error::BadDimensions)
    );
    assert_eq!(
        open(bmp(2049, 8, 24, 0, 0, &[], &[])).0,
        Err(Error::ImageTooLarge)
    );
    assert_eq!(
        open(bmp(8, 3073, 24, 0, 0, &[], &[])).0,
        Err(Error::ImageTooLarge)
    );
    let mut far = bmp(8, 8, 24, 0, 0, &[], &[]);
    far[10] = 0xFF;
    assert_eq!(open(far).0, Err(Error::SeekPixelDataFailed));
    // The fields read before the error stay.
    let (_, out, _) = open(bmp(3000, 7, 24, 0, 0, &[], &[]));
    assert_eq!((out.width, out.height, out.bits_per_pixel), (3000, 7, 24));
}

#[test]
fn a_file_cut_short_reads_zeros() {
    let full = bmp(4, 4, 8, 0, 2, &[[0, 0, 0, 0], [255, 255, 255, 0]], &[0; 16]);
    for end in 0..full.len() {
        let (result, out, _) = open(full[..end].to_vec());
        if end < 2 {
            assert_eq!(result, Err(Error::NotBmp), "{end}");
        } else if end < 15 {
            assert_eq!(result, Err(Error::DibTooSmall), "{end}");
        }
        if end < 62 && result.is_ok() {
            unreachable!("{end}: {out:?}");
        }
    }
}

#[test]
fn corrupted_files_never_panic() {
    let palette: Vec<[u8; 4]> = (0..16u8).map(|i| [i * 16, i, 255 - i, 0]).collect();
    let base = bmp(7, 3, 4, 0, 16, &palette, &[0x5A; 12]);
    let mut seed = 0x2545_F491_u32;
    for i in 0..base.len() {
        for _ in 0..8 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let mut copy = base.clone();
            copy[i] = seed.to_le_bytes()[0];
            let (_, out, _) = open(copy);
            let mut lum = [0; 64];
            let _ = out.row_luminance(&[0xFF; 9], (seed % 70) as usize, &mut lum);
        }
    }
}
