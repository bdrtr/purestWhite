extern crate std;

mod cover;

use super::*;
use std::vec;
use std::vec::Vec;

struct Mem<'a>(&'a [u8]);

impl Source for Mem<'_> {
    fn read_at(&mut self, pos: u32, buf: &mut [u8]) -> usize {
        let rest = self.0.get(pos as usize..).unwrap_or_default();
        let n = rest.len().min(buf.len());
        buf[..n].copy_from_slice(&rest[..n]);
        n
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn byte(&mut self) -> u8 {
        self.next().to_le_bytes()[0]
    }
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// `data` as zlib stored blocks of at most `block` bytes.
fn zlib_stored(data: &[u8], block: usize) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut blocks = data.chunks(block).peekable();
    if blocks.peek().is_none() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    while let Some(part) = blocks.next() {
        out.push(u8::from(blocks.peek().is_none()));
        let len = part.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(part);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn chunk(kind: [u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    let mut crc_input = kind.to_vec();
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
    out
}

fn ihdr(width: u32, height: u32, depth: u8, color: u8, interlace: u8) -> Vec<u8> {
    let mut data = width.to_be_bytes().to_vec();
    data.extend_from_slice(&height.to_be_bytes());
    data.extend_from_slice(&[depth, color, 0, 0, interlace]);
    chunk(*b"IHDR", &data)
}

const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Bytes per complete pixel, as the filters count them.
fn filter_step(depth: u8, color: u8) -> usize {
    let channels = match color {
        2 => 3,
        4 => 2,
        6 => 4,
        _ => 1,
    };
    (channels * usize::from(depth) / 8).max(1)
}

/// The filtered rows (type byte first) for raw `rows`, filter `filters[y % len]` on row `y`.
fn filter_rows(rows: &[Vec<u8>], step: usize, filters: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut prev = vec![0; rows[0].len()];
    for (y, row) in rows.iter().enumerate() {
        let filter = filters[y % filters.len()];
        out.push(filter);
        for (x, &v) in row.iter().enumerate() {
            let a = if x >= step { row[x - step] } else { 0 };
            let b = prev[x];
            let c = if x >= step { prev[x - step] } else { 0 };
            let predicted = match filter {
                1 => a,
                2 => b,
                3 => u8::midpoint(a, b),
                4 => paeth(a, b, c),
                _ => 0,
            };
            out.push(v.wrapping_sub(predicted));
        }
        prev.clone_from(row);
    }
    out
}

fn random_rows(rng: &mut Rng, height: usize, pitch: usize) -> Vec<Vec<u8>> {
    // Smooth-ish data, so every predictor gets used.
    (0..height)
        .map(|y| {
            (0..pitch)
                .map(|x| ((x * 7 + y * 13) as u8).wrapping_add(rng.byte() % 9))
                .collect()
        })
        .collect()
}

/// A file with `before` chunks after the header and the data split into IDATs of `split` bytes.
fn file(header: &[u8], before: &[Vec<u8>], data: &[u8], split: usize) -> Vec<u8> {
    let mut out = SIGNATURE.to_vec();
    out.extend_from_slice(header);
    for c in before {
        out.extend_from_slice(c);
    }
    for part in data.chunks(split.max(1)) {
        out.extend_from_slice(&chunk(*b"IDAT", part));
    }
    out.extend_from_slice(&chunk(*b"IEND", b""));
    out
}

struct Decoded {
    rows: Vec<Vec<u8>>,
    result: Result<(), Error>,
}

fn decode(bytes: &[u8]) -> Result<Decoded, Error> {
    let mut png = Png::new_boxed().unwrap();
    let size = bytes.len() as u32;
    png.open(&mut Mem(bytes), size)?;
    let mut rows = Vec::new();
    let result = png.decode(&mut Mem(bytes), |row| {
        assert_eq!(row.y as usize, rows.len());
        rows.push(row.pixels.to_vec());
        // Corrupt data must not run on for ever.
        rows.len() < 100_000
    });
    Ok(Decoded { rows, result })
}

#[test]
fn undoes_every_filter() {
    let mut rng = Rng(1);
    for (depth, color, width) in [
        (8, 2, 13),
        (8, 6, 9),
        (8, 4, 5),
        (2, 0, 23),
        (8, 0, 1),
        (4, 3, 7),
    ] {
        let step = filter_step(depth, color);
        let pitch = if depth < 8 {
            (width * usize::from(depth)).div_ceil(8)
        } else {
            width * step
        };
        let rows = random_rows(&mut rng, 11, pitch);
        for filters in [
            &[0u8][..],
            &[1],
            &[2],
            &[3],
            &[4],
            &[0, 1, 2, 3, 4],
            &[4, 3, 2, 1, 0],
        ] {
            let raw = filter_rows(&rows, step, filters);
            let bytes = file(
                &ihdr(width as u32, 11, depth, color, 0),
                &[],
                &zlib_stored(&raw, 1000),
                1 << 20,
            );
            let decoded = decode(&bytes).unwrap();
            assert_eq!(decoded.result, Ok(()), "{depth}/{color} {filters:?}");
            assert_eq!(decoded.rows, rows, "{depth}/{color} {filters:?}");
        }
    }
}

/// An unknown filter type leaves the row as it is, as PNGdec did.
#[test]
fn leaves_unknown_filters_alone() {
    let rows = vec![vec![1, 2, 3], vec![4, 5, 6]];
    let raw = [&[7u8][..], &rows[0], &[200], &rows[1]].concat();
    let bytes = file(&ihdr(1, 2, 8, 2, 0), &[], &zlib_stored(&raw, 100), 100);
    assert_eq!(decode(&bytes).unwrap().rows, rows);
}

#[test]
fn reads_data_split_across_chunks_and_past_other_chunks() {
    let mut rng = Rng(2);
    let rows = random_rows(&mut rng, 40, 300);
    let data = zlib_stored(&filter_rows(&rows, 3, &[0, 1, 2, 3, 4]), 777);
    let text = chunk(*b"tEXt", &[b'x'; 5000]);
    let small: Vec<Vec<u8>> = (0..30)
        .map(|_| chunk(*b"gAMA", &[0, 0, 0xB1, 0x8F]))
        .collect();
    for split in [1, 7, 2047, 2048, 2049, 4000, 1 << 20] {
        for before in [&[][..], &[text.clone()][..], &small[..]] {
            let bytes = file(&ihdr(100, 40, 8, 2, 0), before, &data, split);
            let decoded = decode(&bytes).unwrap();
            assert_eq!(decoded.result, Ok(()), "split {split}");
            assert_eq!(decoded.rows, rows, "split {split}");
        }
    }
}

#[test]
fn hands_out_the_palette_and_color_keys() {
    let palette: Vec<u8> = (0..48).map(|i| (i * 5) as u8).collect();
    let rows = vec![vec![0x01, 0x23], vec![0x45, 0x67]];
    let data = zlib_stored(&filter_rows(&rows, 1, &[0]), 100);
    let header = ihdr(4, 2, 4, 3, 0);
    let plte = chunk(*b"PLTE", &palette);
    let trns = chunk(*b"tRNS", &[10, 20, 30]);

    let bytes = file(&header, &[plte.clone(), trns], &data, 100);
    let mut png = Png::new_boxed().unwrap();
    png.open(&mut Mem(&bytes), bytes.len() as u32).unwrap();
    assert!(!png.has_alpha());
    let mut seen = 0;
    let result = png.decode(&mut Mem(&bytes), |row| {
        assert_eq!(&row.palette[..48], &palette[..]);
        assert_eq!(row.palette[48], 0);
        assert_eq!(&row.palette[768..772], &[10, 20, 30, 0xFF]);
        assert!(row.has_alpha);
        seen += 1;
        true
    });
    assert_eq!((result, seen), (Ok(()), 2));
    assert!(png.has_alpha());

    // Color keys: the low byte of each 16-bit sample.
    for (color, key, expected) in [
        (0u8, &[0x12, 0x34][..], 0x34),
        (2, &[1, 2, 3, 4, 5, 6][..], 0x02_04_06),
    ] {
        let channels = if color == 2 { 3 } else { 1 };
        let rows = vec![vec![9; 4 * channels]];
        let data = zlib_stored(&filter_rows(&rows, channels, &[0]), 100);
        let bytes = file(
            &ihdr(4, 1, 8, color, 0),
            &[chunk(*b"tRNS", key)],
            &data,
            100,
        );
        let mut png = Png::new_boxed().unwrap();
        png.open(&mut Mem(&bytes), bytes.len() as u32).unwrap();
        let result = png.decode(&mut Mem(&bytes), |row| {
            assert!(row.has_alpha);
            assert_eq!(row.transparent, expected);
            true
        });
        assert_eq!(result, Ok(()));
    }
}

/// A palette or tRNS chunk longer than the palette is cut, where PNGdec wrote on past it.
#[test]
fn cuts_long_palettes() {
    let palette: Vec<u8> = (0..1500).map(|i| i as u8).collect();
    let alpha: Vec<u8> = (0..400).map(|i| (i * 3) as u8).collect();
    let rows = vec![vec![255]];
    let data = zlib_stored(&filter_rows(&rows, 1, &[0]), 100);
    let bytes = file(
        &ihdr(1, 1, 8, 3, 0),
        &[chunk(*b"PLTE", &palette), chunk(*b"tRNS", &alpha)],
        &data,
        100,
    );
    let mut png = Png::new_boxed().unwrap();
    png.open(&mut Mem(&bytes), bytes.len() as u32).unwrap();
    let result = png.decode(&mut Mem(&bytes), |row| {
        assert_eq!(&row.palette[..768], &palette[..768]);
        assert_eq!(&row.palette[768..], &alpha[..256]);
        true
    });
    assert_eq!(result, Ok(()));
}

#[test]
fn stops_when_asked() {
    let rows = vec![vec![1; 3]; 5];
    let data = zlib_stored(&filter_rows(&rows, 3, &[0]), 100);
    let bytes = file(&ihdr(1, 5, 8, 2, 0), &[], &data, 100);
    let mut png = Png::new_boxed().unwrap();
    png.open(&mut Mem(&bytes), bytes.len() as u32).unwrap();
    let mut seen = 0;
    let result = png.decode(&mut Mem(&bytes), |_| {
        seen += 1;
        seen < 2
    });
    assert_eq!((result, seen), (Err(Error::QuitEarly), 2));
}

#[test]
fn refuses_what_it_cannot_decode() {
    let good = file(&ihdr(4, 4, 8, 2, 0), &[], &zlib_stored(&[0; 52], 100), 100);
    let open = |bytes: &[u8]| {
        let mut png = Png::new_boxed().unwrap();
        let result = png.open(&mut Mem(bytes), bytes.len() as u32);
        (result, png.width(), png.height())
    };
    assert_eq!(open(&good), (Ok(()), 4, 4));
    assert_eq!(open(&good[..32]).0, Err(Error::InvalidFile));
    let mut bad = good.clone();
    bad[3] = b'X';
    assert_eq!(open(&bad).0, Err(Error::InvalidFile));
    let mut bad = good.clone();
    bad[29] ^= 1; // the IHDR CRC
    assert_eq!(open(&bad).0, Err(Error::InvalidFile));
    // The fields are set before these.
    assert_eq!(
        open(&file(&ihdr(4, 4, 8, 2, 1), &[], &[], 1)),
        (Err(Error::Unsupported), 4, 4)
    );
    assert_eq!(
        open(&file(&ihdr(4, 4, 16, 0, 0), &[], &[], 1)).0,
        Err(Error::Unsupported)
    );
    assert_eq!(
        open(&file(&ihdr(5472, 1, 8, 2, 0), &[], &[], 1)),
        (Err(Error::TooBig), 5472, 1)
    );
    assert_eq!(open(&file(&ihdr(5471, 1, 8, 2, 0), &[], &[], 1)).0, Ok(()));
    // No IHDR first: everything stays 0.
    let mut moved = SIGNATURE.to_vec();
    moved.extend_from_slice(&chunk(*b"gAMA", &[0; 4]));
    moved.extend_from_slice(&good[8..]);
    assert_eq!(open(&moved), (Ok(()), 0, 0));

    let mut png = Png::new_boxed().unwrap();
    assert_eq!(png.decode(&mut Mem(&good), |_| true), Err(Error::NotOpen));
}

/// PNGdec waited forever for data that never came; here every cut ends.
#[test]
fn every_cut_ends() {
    let mut rng = Rng(3);
    let rows = random_rows(&mut rng, 6, 30);
    let data = zlib_stored(&filter_rows(&rows, 3, &[0, 1, 2, 3, 4]), 50);
    let bytes = file(
        &ihdr(10, 6, 8, 2, 0),
        &[chunk(*b"tEXt", b"a\0b")],
        &data,
        60,
    );
    for cut in 33..bytes.len() {
        let decoded = decode(&bytes[..cut]).unwrap();
        assert!(rows.starts_with(&decoded.rows), "cut {cut}");
        if decoded.result == Ok(()) {
            assert_eq!(decoded.rows, rows, "cut {cut}");
        }
    }
    // Cut inside image data.
    let idat = bytes.windows(4).position(|w| w == b"IDAT").unwrap();
    assert_eq!(
        decode(&bytes[..idat + 9]).unwrap().result,
        Err(Error::FileEnded)
    );
}

/// A stream that ends inside a row ends the decode, where PNGdec walked on to the end of the
/// file; one that ends on a row boundary short of the height is complete, as PNGdec had it.
#[test]
fn a_short_stream_ends_the_decode() {
    let rows = vec![vec![5; 6]; 3];
    let raw = filter_rows(&rows, 3, &[0]);
    let cut = file(
        &ihdr(2, 3, 8, 2, 0),
        &[],
        &zlib_stored(&raw[..10], 100),
        100,
    );
    let decoded = decode(&cut).unwrap();
    assert_eq!(
        (decoded.rows.len(), decoded.result),
        (1, Err(Error::FileEnded))
    );

    let short = file(&ihdr(2, 5, 8, 2, 0), &[], &zlib_stored(&raw, 100), 100);
    let decoded = decode(&short).unwrap();
    assert_eq!((decoded.rows, decoded.result), (rows, Ok(())));
}

/// Damage anywhere ends in a result, never a panic or a hang, with rows no longer than a row.
#[test]
fn survives_random_damage() {
    let mut rng = Rng(4);
    let rows = random_rows(&mut rng, 20, 60);
    let raw = filter_rows(&rows, 3, &[0, 1, 2, 3, 4]);
    let mut seeds = vec![
        file(&ihdr(20, 20, 8, 2, 0), &[], &zlib_stored(&raw, 300), 200),
        file(
            &ihdr(16, 8, 4, 3, 0),
            &[chunk(*b"PLTE", &[7; 48]), chunk(*b"tRNS", &[1, 2])],
            &zlib_stored(&raw[..72], 30),
            25,
        ),
    ];
    // Four 40-pixel gray rows (filters 0, 1, 2, 4) as zlib level 9 wrote them: Huffman codes and
    // matches, for damage to land in.
    let coded = [
        120, 218, 99, 96, 96, 102, 99, 98, 101, 100, 33, 76, 177, 18, 9, 152, 24, 137, 4, 44, 12,
        140, 76, 204, 196, 16, 0, 165, 197, 1, 193,
    ];
    let decoded = decode(&file(&ihdr(40, 4, 8, 0, 0), &[], &coded, 9)).unwrap();
    assert_eq!((decoded.rows.len(), decoded.result), (4, Ok(())));
    seeds.push(file(&ihdr(40, 4, 8, 0, 0), &[], &coded, 9));
    for seed in &seeds {
        for _ in 0..3000 {
            let mut bytes = seed.clone();
            for _ in 0..=rng.below(4) {
                if bytes.len() <= 8 {
                    break;
                }
                let at = 8 + rng.below(bytes.len() - 8);
                match rng.below(3) {
                    0 => bytes[at] ^= 1 << rng.below(8),
                    1 => bytes[at] = rng.byte(),
                    _ => bytes.truncate(at),
                }
            }
            let mut png = Png::new_boxed().unwrap();
            if png.open(&mut Mem(&bytes), bytes.len() as u32).is_err() {
                continue;
            }
            let pitch = png.pitch;
            let mut count = 0;
            let _ = png.decode(&mut Mem(&bytes), |row| {
                assert_eq!(row.pixels.len() as i32, pitch);
                count += 1;
                count < 10_000
            });
        }
    }
}

/// Flat 31-pixel rows, each Up from the last, as zlib level 9 wrote them. PNGdec built for a 64-bit
/// host decoded every row's first pixel 40 too high: its zlib wrote the run that ends a row 4 bytes
/// at a time, past the row's end into the previous row, which Up then added. Its 32-bit firmware
/// build copied byte by byte and got them right, as this does.
#[test]
fn rows_ending_in_a_run_are_exact() {
    let coded = [
        120, 218, 99, 210, 192, 15, 152, 104, 44, 15, 0, 219, 60, 19, 105,
    ];
    let decoded = decode(&file(&ihdr(31, 4, 8, 0, 0), &[], &coded, 100)).unwrap();
    assert_eq!(decoded.result, Ok(()));
    let expected: Vec<Vec<u8>> = [40, 80, 120, 160].iter().map(|&v| vec![v; 31]).collect();
    assert_eq!(decoded.rows, expected);
}

/// A stream that ends after the last row, but in a later piece than the row: a 2026-pixel gray
/// image in one stored block, so the IDAT reads go 2015, 2048 and 2 bytes and the last two bytes
/// of the Adler-32 trailer come alone. PNGdec read the rest of the chunk and succeeded.
#[test]
fn a_trailer_after_the_last_row_is_success() {
    let rows: Vec<Vec<u8>> = (0..2)
        .map(|y| (0..2026).map(|x| ((x * 7 + y) % 256) as u8).collect())
        .collect();
    let data = zlib_stored(&filter_rows(&rows, 1, &[0]), 65535);
    assert_eq!(data.len(), 4065);
    let bytes = file(&ihdr(2026, 2, 8, 0, 0), &[], &data, 1 << 20);
    let decoded = decode(&bytes).unwrap();
    assert_eq!((decoded.rows, decoded.result), (rows, Ok(())));
}

/// Data for part of a row past the height, in the IDAT that holds the last row, ends the decode
/// as a success, as in PNGdec.
#[test]
fn a_partial_row_past_the_height_is_success() {
    let rows: Vec<Vec<u8>> = (0..2)
        .map(|y| (0..2026).map(|x| ((x * 7 + y) % 256) as u8).collect())
        .collect();
    let raw = filter_rows(&rows, 1, &[0]);
    let bytes = file(
        &ihdr(2026, 1, 8, 0, 0),
        &[],
        &zlib_stored(&raw[..2027 + 1000], 65535),
        1 << 20,
    );
    let decoded = decode(&bytes).unwrap();
    assert_eq!((decoded.rows, decoded.result), (rows[..1].to_vec(), Ok(())));
}
