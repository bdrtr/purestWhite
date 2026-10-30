//! Pages read from a card in memory (formatted by mkfs.fat), drawn into a frame as upright
//! 480 by 800 images: in each depth and row order, and refused when they do not fit.

use crate::pages::{MAX_PAGES, PageError, PagePath, count_pages, load_page};
use embassy_futures::block_on;
use purestwhite_fat::Volume;
use purestwhite_fat::testing::{MemCard, ready, test_card};
use purestwhite_render::{FRAME_BYTES, HEIGHT, WIDTH, is_black};
use std::boxed::Box;
use std::vec;
use std::vec::Vec;

/// The luminance of the test pattern at (`x`, `y`): stripes, a diagonal, and a gray ramp.
fn pattern(x: i32, y: i32) -> u8 {
    if (x + 2 * y) % 37 < 5 || x == y {
        0
    } else {
        u8::try_from((x * 255 / WIDTH + y) % 256).unwrap()
    }
}

/// A BMP of `width` by `height` at `bpp` (1 with a black and white palette, or 24), whose pixels
/// have the luminance `lum(x, y)` upright; rows bottom-up unless `top_down`.
fn bmp(bpp: u16, top_down: bool, width: i32, height: i32, lum: impl Fn(i32, i32) -> u8) -> Vec<u8> {
    let row_bytes = (width * i32::from(bpp) + 31) / 32 * 4;
    let palette: &[u8] = if bpp == 1 {
        &[0, 0, 0, 0, 255, 255, 255, 0]
    } else {
        &[]
    };
    let offset = 54 + palette.len();
    let mut b = vec![0u8; offset];
    b[..2].copy_from_slice(b"BM");
    b[10..14].copy_from_slice(&u32::try_from(offset).unwrap().to_le_bytes());
    b[14..18].copy_from_slice(&40u32.to_le_bytes());
    b[18..22].copy_from_slice(&width.to_le_bytes());
    b[22..26].copy_from_slice(&(if top_down { -height } else { height }).to_le_bytes());
    b[26..28].copy_from_slice(&1u16.to_le_bytes());
    b[28..30].copy_from_slice(&bpp.to_le_bytes());
    b[54..].copy_from_slice(palette);
    for file_row in 0..height {
        let y = if top_down {
            file_row
        } else {
            height - 1 - file_row
        };
        let mut row = vec![0u8; usize::try_from(row_bytes).unwrap()];
        for x in 0..width {
            let l = lum(x, y);
            let xi = usize::try_from(x).unwrap();
            if bpp == 1 {
                row[xi / 8] |= u8::from(l >= 128) << (7 - xi % 8);
            } else {
                row[xi * 3..xi * 3 + 3].fill(l);
            }
        }
        b.extend_from_slice(&row);
    }
    b
}

/// A card with the pages `pages` (`(n, bytes)`) in `purestwhite-pages/`.
fn card(pages: &[(u16, Vec<u8>)]) -> Option<Volume<MemCard>> {
    let mut volume = ready(Volume::mount(test_card(4096)?)).unwrap().unwrap();
    ready(volume.mkdir("purestwhite-pages")).unwrap().unwrap();
    for (n, bytes) in pages {
        let mut file = ready(volume.create(PagePath::new(*n).as_str()))
            .unwrap()
            .unwrap();
        assert_eq!(
            ready(volume.write(&mut file, bytes)).unwrap().unwrap(),
            bytes.len()
        );
        ready(volume.close(file)).unwrap().unwrap();
    }
    Some(volume)
}

fn frame() -> Box<[u8; FRAME_BYTES]> {
    vec![0u8; FRAME_BYTES]
        .into_boxed_slice()
        .try_into()
        .unwrap()
}

/// Page 1 of `volume` read into a frame (it yields between pieces, so it is run to the end).
fn load(volume: &mut Volume<MemCard>) -> (Result<(), PageError<()>>, Box<[u8; FRAME_BYTES]>) {
    let mut frame = frame();
    let loaded = block_on(load_page(volume, 1, &mut frame));
    (loaded, frame)
}

/// Whether `frame` is black exactly where `pattern` is dark.
fn shows_pattern(frame: &[u8; FRAME_BYTES]) -> bool {
    (0..HEIGHT).all(|y| (0..WIDTH).all(|x| is_black(frame, x, y) == (pattern(x, y) < 128)))
}

#[test]
fn a_page_path_is_numbered_from_one() {
    assert_eq!(PagePath::new(1).as_str(), "purestwhite-pages/001.bmp");
    assert_eq!(PagePath::new(42).as_str(), "purestwhite-pages/042.bmp");
    assert_eq!(
        PagePath::new(MAX_PAGES).as_str(),
        "purestwhite-pages/999.bmp"
    );
}

#[test]
fn pages_are_counted_until_one_is_missing() {
    let page = bmp(1, false, WIDTH, HEIGHT, pattern);
    let pages: Vec<(u16, Vec<u8>)> = [1, 2, 3, 5].map(|n| (n, page.clone())).into();
    let Some(mut volume) = card(&pages) else {
        return;
    };
    assert_eq!(ready(count_pages(&mut volume)).unwrap(), 3);
    let Some(mut empty) = card(&[]) else { return };
    assert_eq!(ready(count_pages(&mut empty)).unwrap(), 0);
}

#[test]
fn every_depth_and_row_order_draws_the_same_page() {
    for (bpp, top_down) in [(1, false), (1, true), (24, false), (24, true)] {
        let Some(mut volume) = card(&[(1, bmp(bpp, top_down, WIDTH, HEIGHT, pattern))]) else {
            return;
        };
        let (loaded, frame) = load(&mut volume);
        assert_eq!(loaded, Ok(()), "{bpp} bits, top down {top_down}");
        assert!(shows_pattern(&frame), "{bpp} bits, top down {top_down}");
    }
}

#[test]
fn a_page_of_another_size_is_refused() {
    for (width, height) in [(800, 480), (480, 799), (481, 800)] {
        let Some(mut volume) = card(&[(1, bmp(1, false, width, height, pattern))]) else {
            return;
        };
        assert_eq!(
            load(&mut volume).0,
            Err(PageError::Size),
            "{width}x{height}"
        );
    }
}

#[test]
fn a_missing_short_or_foreign_file_is_an_error() {
    let Some(mut volume) = card(&[]) else { return };
    assert_eq!(
        load(&mut volume).0,
        Err(PageError::File(purestwhite_fat::Error::NotFound))
    );
    let mut short = bmp(1, false, WIDTH, HEIGHT, pattern);
    short.truncate(short.len() - 10);
    let Some(mut volume) = card(&[(1, short)]) else {
        return;
    };
    assert_eq!(load(&mut volume).0, Err(PageError::Truncated));
    let Some(mut volume) = card(&[(1, vec![0x42; 2000])]) else {
        return;
    };
    assert_eq!(
        load(&mut volume).0,
        Err(PageError::Bmp(bembeyaz_bmp::Error::NotBmp))
    );
}
