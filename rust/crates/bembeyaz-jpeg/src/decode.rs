//! The decode itself: JPEGDEC's `DecodeJPEG`, `JPEGDecodeMCU(_P)`, `JPEGIDCT` and `JPEGPutMCU8BitGray`,
//! in its 32-bit integer arithmetic (`Wrapping` where its `int`s could overflow) and with its
//! truncating casts.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::similar_names,
    reason = "JPEGDEC's C casts, wrapping int arithmetic and names, on purpose"
)]

use core::num::Wrapping;
use core::num::Wrapping as W;

use crate::tables::{RANGE, SCALE_BITS, UNZIGZAG, ZIGZAG};
use crate::{
    Block, DC_BASE, DC_TABLE_SIZE, DCTSIZE, Error, FILE_HIGHWATER, HUFF11SIZE, Jpeg,
    MAX_BUFFERED_PIXELS, MAX_MCUS, MCU_BASE, MCU_SKIP, Scale, Source, scale,
};

/// The luma and chroma blocks of an MCU, as offsets (in i16) into the MCU area.
const MCU0: i32 = 0;
const MCU1: i32 = 64;
const MCU2: i32 = 128;
const MCU3: i32 = 192;

/// A result of an IDCT stage clamped to a pixel.
#[inline(always)]
fn range(value: W<i32>) -> u8 {
    RANGE
        .get(((value >> 5).0 & 0x3ff) as usize)
        .copied()
        .unwrap_or(0)
}

impl Jpeg {
    // The MCU area: i16 blocks the IDCT overwrites with bytes.

    /// The i16 at `index` (in i16, from MCU0; `MCU_SKIP`'s -8 is the area's start).
    #[inline(always)]
    fn coeff(&self, index: i32) -> i16 {
        let at = mcu_at(index);
        match self.mcus.get(at..at + 2) {
            Some(&[a, b]) => i16::from_le_bytes([a, b]),
            _ => 0,
        }
    }

    #[inline(always)]
    fn set_coeff(&mut self, index: i32, value: i16) {
        let at = mcu_at(index);
        if let Some(slot) = self.mcus.get_mut(at..at + 2) {
            slot.copy_from_slice(&value.to_le_bytes());
        }
    }

    /// Fills `len` bytes of the MCU area from byte `at`.
    #[inline(always)]
    fn fill_mcu(&mut self, at: usize, len: usize, value: u8) {
        if let Some(slot) = self.mcus.get_mut(at..at + len) {
            slot.fill(value);
        }
    }

    #[inline(always)]
    fn quant_at(&self, index: usize) -> i32 {
        i32::from(self.quant.get(index).copied().unwrap_or(0) as i16)
    }

    /// Decodes the image opened from `source` at `size` and hands each block to `draw`, which
    /// returns false to stop (the decode then still succeeds, as in JPEGDEC).
    ///
    /// # Errors
    /// [`Error::NotOpen`] when the image did not open or was decoded already; otherwise why the
    /// scan could not be decoded (the blocks before it were drawn).
    #[allow(
        clippy::too_many_lines,
        reason = "JPEGDEC's DecodeJPEG in one piece, to read against it"
    )]
    pub fn decode(
        &mut self,
        source: &mut impl Source,
        size: Scale,
        mut draw: impl FnMut(&Block<'_>) -> bool,
    ) -> Result<(), Error> {
        if !self.opened {
            return Err(Error::NotOpen);
        }
        self.opened = false;
        self.options = match size {
            Scale::Full => 0,
            Scale::Half => scale::HALF,
            Scale::Quarter => scale::QUARTER,
            Scale::Eighth => scale::EIGHTH,
        };
        let mut max_fill = 16;
        let mut scale_shift = 0;
        let mut thumbnail = false;
        if self.mode == 0xc2 {
            // Progressive: only the first scan (the DC values), at 1/8 size unless asked otherwise.
            self.options |= scale::EIGHTH;
        }
        if self.options & scale::HALF != 0 {
            scale_shift = 1;
        } else if self.options & scale::QUARTER != 0 {
            scale_shift = 2;
            max_fill = 1;
        } else if self.options & scale::EIGHTH != 0 {
            scale_shift = 3;
            max_fill = 1;
            thumbnail = true;
        }
        self.fix_quant();
        self.bits = self.be32(0);
        self.bit_buf = 0;
        self.bit_off = 0;

        let [c0, c1, c2, _] = self.components;
        let (mut pred0, mut pred1, mut pred2) = (0i32, 0i32, 0i32);
        let (cx, cy, mut mcu_cx, mut mcu_cy) = match self.sub_sample {
            0x00 | 0x01 | 0x11 => ((self.width + 7) >> 3, (self.height + 7) >> 3, 8, 8),
            0x12 => ((self.width + 7) >> 3, (self.height + 15) >> 4, 8, 16),
            0x21 => ((self.width + 15) >> 4, (self.height + 7) >> 3, 16, 8),
            0x22 => ((self.width + 15) >> 4, (self.height + 15) >> 4, 16, 16),
            // JPEGDEC divided by a zero MCU size here.
            _ => return Err(Error::Unsupported),
        };
        mcu_cx >>= scale_shift;
        mcu_cy >>= scale_shift;
        let quant1 = self.quant_at(usize::from(c0.quant) * DCTSIZE);
        self.res_count = self.res_interval;
        // As many MCUs as the pixel buffer holds, at one byte a pixel.
        let mcu_count = (MAX_BUFFERED_PIXELS / (mcu_cx * mcu_cy) * 2)
            .min(cx)
            .min(MAX_MCUS);
        let mut block_height = mcu_cy;
        let mut pitch;
        let mut more = true;
        let mut err = 0;
        let mut y = 0;
        while y < cy && more && err == 0 {
            let mut block_x = 0;
            let mut xoff = 0;
            pitch = mcu_count * mcu_cx;
            let mut x = 0;
            while x < cx && more && err == 0 {
                self.ac_table = c0.ac;
                self.dc_table = c0.dc;
                // The luma blocks.
                let luma = if self.sub_sample > 0x11 {
                    if self.sub_sample == 0x22 { 4 } else { 2 }
                } else {
                    1
                };
                for (i, block) in [MCU0, MCU1, MCU2, MCU3].into_iter().enumerate() {
                    if i >= luma {
                        break;
                    }
                    let failed = if self.mode == 0xc2 {
                        self.decode_mcu_p(block, &mut pred0)
                    } else {
                        self.decode_mcu(block, &mut pred0)
                    };
                    if i == 0 {
                        err = i32::from(failed);
                    } else {
                        err |= i32::from(failed);
                    }
                    if self.mcu_flags == 0 || thumbnail {
                        // No AC values: the block is its DC value.
                        let c = range(Wrapping(pred0) * Wrapping(quant1));
                        self.fill_mcu(mcu_at(block), 4 * max_fill, c);
                    } else {
                        self.idct(block, usize::from(c0.quant), failed);
                    }
                }
                if self.sub_sample != 0 && self.num_components == 3 {
                    // Color drawn as gray: decode the chroma blocks only to get past them.
                    self.ac_table = c1.ac;
                    self.dc_table = c1.dc;
                    if self.mode == 0xc2 {
                        // A progressive scan may hold luma without chroma.
                        if self.components[1].needed {
                            err |= i32::from(self.decode_mcu_p(MCU_SKIP, &mut pred1));
                        }
                        if self.components[2].needed {
                            self.ac_table = c2.ac;
                            self.dc_table = c2.dc;
                            err |= i32::from(self.decode_mcu_p(MCU_SKIP, &mut pred2));
                        }
                    } else {
                        err |= i32::from(self.decode_mcu(MCU_SKIP, &mut pred1));
                        err |= i32::from(self.decode_mcu(MCU_SKIP, &mut pred2));
                    }
                }
                self.put_mcu_gray(xoff, pitch);
                xoff += mcu_cx;
                if xoff == pitch || x == cx - 1 {
                    let adjust = (1 << scale_shift) - 1;
                    let cur_w = (self.width + adjust) >> scale_shift;
                    let cur_h = (self.height + adjust) >> scale_shift;
                    let mut width_used = pitch;
                    if block_x + pitch > cur_w {
                        width_used = cur_w - block_x;
                    }
                    let block_y = y * mcu_cy;
                    if block_y + mcu_cy > cur_h {
                        block_height = cur_h - block_y;
                    }
                    more = draw(&Block {
                        x: block_x,
                        y: block_y,
                        width: pitch,
                        height: block_height,
                        width_used,
                        pixels: &self.pixels,
                    });
                    block_x += pitch;
                    // JPEGDEC's crop to the image width comes to the same pitch.
                    if cx - 1 - x < mcu_count {
                        pitch = (cx - 1 - x) * mcu_cx;
                    }
                    xoff = 0;
                    if pitch & (mcu_cx - 1) != 0 {
                        // The MCUs are drawn whole: round the block up.
                        pitch = (pitch + (mcu_cx - 1)) & !(mcu_cx - 1);
                    }
                }
                if self.res_interval != 0 {
                    self.res_count -= 1;
                    if self.res_count == 0 {
                        self.res_count = self.res_interval;
                        pred0 = 0;
                        pred1 = 0;
                        pred2 = 0;
                        if self.bit_off & 7 != 0 {
                            // A restart interval starts on a byte boundary.
                            self.bit_off += 8 - (self.bit_off & 7);
                        }
                    }
                }
                if self.vlc_off >= FILE_HIGHWATER {
                    self.get_more_data(source);
                }
                x += 1;
            }
            y += 1;
        }
        if err != 0 {
            return Err(Error::Decode);
        }
        Ok(())
    }

    /// Reorders the quantization tables to natural order and prescales them for the IDCT. As
    /// JPEGDEC, it fixes as many tables as the frame has components, from table 0 (four at most).
    fn fix_quant(&mut self) {
        for table in 0..usize::from(self.num_components).min(4) {
            let base = table * DCTSIZE;
            let mut reordered = [0u16; DCTSIZE];
            for (i, slot) in reordered.iter_mut().enumerate() {
                let from = base + usize::from(ZIGZAG.get(i).copied().unwrap_or(0));
                *slot = self.quant.get(from).copied().unwrap_or(0);
            }
            for (i, value) in reordered.into_iter().enumerate() {
                let scaled = (i32::from(value) * SCALE_BITS.get(i).copied().unwrap_or(0)) >> 12;
                if let Some(q) = self.quant.get_mut(base + i) {
                    *q = scaled as u16;
                }
            }
        }
    }

    /// Tops the bit buffer up when more than 15 of its bits are used.
    #[inline(always)]
    fn refill(&mut self, bit_off: &mut u32, bits: &mut u32, buf: &mut usize) {
        if *bit_off > 15 {
            *buf += (*bit_off >> 3) as usize;
            *bit_off &= 7;
            *bits = self.be32(*buf);
        }
    }

    /// The `size` extra bits at `bit_off` as a signed value (JPEGDEC's sign-sliding).
    #[inline(always)]
    fn extra(bits: u32, bit_off: u32, size: u32) -> u32 {
        let code = bits.wrapping_shl(bit_off);
        let sign = !(((code as i32) >> 31) as u32);
        (code >> (32 - size)).wrapping_sub(sign >> (32 - size))
    }

    /// Decodes one baseline block into MCU `mcu` (or, for `MCU_SKIP`, only reads past it); true on
    /// an invalid code.
    fn decode_mcu(&mut self, mcu: i32, predictor: &mut i32) -> bool {
        let mut bit_off = self.bit_off;
        let mut bits = self.bits;
        let mut buf = self.bit_buf;
        self.refill(&mut bit_off, &mut bits, &mut buf);
        // Coefficients stored: none past DC when skipped, four for 1/4 and 1/8.
        let end2 = if mcu < 0 {
            1
        } else if self.options & (scale::QUARTER | scale::EIGHTH) != 0 {
            for i in [1, 8, 9] {
                self.set_coeff(mcu + i, 0);
            }
            5
        } else {
            self.fill_mcu(mcu_at(mcu), 2 * DCTSIZE, 0);
            64
        };
        let mut flags: u16 = 0;
        let mut zig: usize = 1;

        // The DC value.
        let table = DC_BASE + usize::from(self.dc_table) * DC_TABLE_SIZE;
        let mut code = (bits >> (32 - 12 - bit_off)) & 0xfff;
        code = if code >= 0xf80 {
            code & 0xff
        } else {
            code >> 6
        };
        let mut huff = self.byte(table + code as usize);
        let small = self.byte(table + code as usize + 512) as i8;
        if huff == 0 {
            return true;
        }
        bit_off += u32::from(huff >> 4);
        huff &= 0xf;
        if huff != 0 {
            if small != 0 {
                *predictor = predictor.wrapping_add(i32::from(small));
            } else {
                self.refill(&mut bit_off, &mut bits, &mut buf);
                let value = Self::extra(bits, bit_off, u32::from(huff));
                bit_off += u32::from(huff);
                *predictor = predictor.wrapping_add(value as i32);
            }
        }
        if mcu >= 0 {
            self.set_coeff(mcu, *predictor as i16);
        }
        if self.ac_table > 1 {
            return true;
        }
        if self.scan_end == 0 {
            // DC values only. JPEGDEC returned here without keeping its place in the bits.
            return false;
        }
        // The 63 AC values, through the 10-bit tables.
        let ac = usize::from(self.ac_table) * HUFF11SIZE;
        while zig < 64 {
            self.refill(&mut bit_off, &mut bits, &mut buf);
            let mut code = (bits >> (32 - 16 - bit_off)) & 0xffff;
            code = if code >= 0xfc00 {
                code & 0x7ff
            } else {
                code >> 6
            };
            let mut huff = u32::from(self.ac(ac + code as usize));
            if huff == 0 {
                return true;
            }
            bit_off += huff >> 8;
            huff &= 0xff;
            if huff == 0 {
                // End of block.
                break;
            }
            zig += (huff >> 4) as usize;
            huff &= 0xf;
            if zig < end2 && huff != 0 {
                let value = Self::extra(bits, bit_off, huff);
                let at = UNZIGZAG.get(zig).copied().unwrap_or(0);
                flags |= 1 << (at & 7);
                flags |= u16::from(at) << 8;
                self.set_coeff(mcu + i32::from(at), value as i16);
            }
            bit_off += huff;
            zig += 1;
            self.refill(&mut bit_off, &mut bits, &mut buf);
        }
        self.bit_buf = buf;
        self.vlc_off = buf as i32;
        self.bit_off = bit_off;
        self.bits = bits;
        self.mcu_flags = flags;
        false
    }

    /// Decodes one block of a progressive scan into MCU `mcu` (for `MCU_SKIP`, its AC values into
    /// block 0 and its DC value nowhere, as CrossPoint's JPEGDEC patches have it); true on an
    /// invalid code.
    #[allow(
        clippy::too_many_lines,
        reason = "JPEGDEC's JPEGDecodeMCU_P in one piece, to read against it"
    )]
    fn decode_mcu_p(&mut self, mcu: i32, predictor: &mut i32) -> bool {
        // The patched JPEGDEC writes a skipped block's values to block 0 (and its DC nowhere).
        let block = if mcu < 0 { 0 } else { mcu & 0xff_ffff };
        let mut bit_off = self.bit_off;
        let mut bits = self.bits;
        let mut buf = self.bit_buf;
        self.refill(&mut bit_off, &mut bits, &mut buf);
        let positive = 1i32 << self.approx_low;
        let negative = (-1i32).wrapping_shl(u32::from(self.approx_low));
        'done: {
            if self.scan_start == 0 {
                if self.approx_high != 0 {
                    // Successive approximation: one more bit of the DC value.
                    let code = (bits >> (31 - bit_off)) & 1;
                    bit_off += 1;
                    if code != 0 && mcu >= 0 {
                        let value = i32::from(self.coeff(block)) | positive;
                        self.set_coeff(block, value as i16);
                    }
                    break 'done;
                }
                let table = DC_BASE + usize::from(self.dc_table) * DC_TABLE_SIZE;
                let mut code = (bits >> (32 - 12 - bit_off)) & 0xfff;
                code = if code >= 0xf80 {
                    code & 0xff
                } else {
                    code >> 6
                };
                let mut huff = self.byte(table + code as usize);
                if huff == 0 {
                    return true;
                }
                bit_off += u32::from(huff >> 4);
                huff &= 0xf;
                if huff != 0 {
                    self.refill(&mut bit_off, &mut bits, &mut buf);
                    let value = Self::extra(bits, bit_off, u32::from(huff));
                    bit_off += u32::from(huff);
                    let value = value.wrapping_shl(u32::from(self.approx_low));
                    *predictor = predictor.wrapping_add(value as i32);
                }
                if mcu >= 0 {
                    self.set_coeff(block, *predictor as i16);
                }
            }
            // The AC values of this band.
            let ac = usize::from(self.ac_table) * HUFF11SIZE;
            let mut index = if self.scan_start != 0 {
                i32::from(self.scan_start)
            } else {
                1
            };
            let scan_end = i32::from(self.scan_end);
            if self.approx_high != 0 {
                // Successive approximation: refine the values already there.
                while index <= scan_end {
                    self.refill(&mut bit_off, &mut bits, &mut buf);
                    let mut code = (bits >> (32 - 16 - bit_off)) & 0xffff;
                    code = if code >= 0xf000 {
                        code & 0x1fff
                    } else {
                        code >> 4
                    };
                    let mut huff = u32::from(self.ac(ac + code as usize));
                    if huff == 0 {
                        return true;
                    }
                    bit_off += huff >> 8;
                    huff &= 0xff;
                    let mut coeff = 0;
                    if huff & 0xf != 0 {
                        if huff & 0xf != 1 {
                            return true;
                        }
                        // A progressive image's AC tables are never built, and DC tables 2 and 3
                        // lie where they would be (as in JPEGDEC), so an AC lookup can read a
                        // length that leaves `bit_off` past 31. JPEGDEC's unsigned shift amount
                        // then wrapped and the CPU used its low 5 bits, as `wrapping_shr` does.
                        let code = bits.wrapping_shr(31u32.wrapping_sub(bit_off)) & 1;
                        bit_off += 1;
                        coeff = if code != 0 { positive } else { negative };
                    } else if huff != 0xf0 {
                        // End of band: its run count is not used.
                        bit_off += huff >> 4;
                        break;
                    }
                    let mut count = (huff >> 4) as i32;
                    loop {
                        let value = self.coeff(block + index);
                        if value != 0 {
                            self.refill(&mut bit_off, &mut bits, &mut buf);
                            let code = (bits >> (32 - 1 - bit_off)) & 1;
                            bit_off += 1;
                            if code != 0 && i32::from(value) & positive == 0 {
                                let delta = if value >= 0 { positive } else { negative };
                                self.set_coeff(block + index, value.wrapping_add(delta as i16));
                            }
                        } else {
                            count -= 1;
                            if count < 0 {
                                break;
                            }
                        }
                        index += 1;
                        if index > scan_end {
                            break;
                        }
                    }
                    if coeff != 0 && index < 0x40 {
                        self.set_coeff(block + index, coeff as i16);
                    }
                    index += 1;
                }
            } else {
                while index <= scan_end {
                    if bit_off > 15 {
                        buf += 2;
                        bit_off -= 16;
                        bits = (bits << 16) | u32::from(self.be16(buf + 2));
                    }
                    // `bit_off` may still be past 16, as above.
                    let mut code = bits.wrapping_shr(16u32.wrapping_sub(bit_off)) & 0xffff;
                    code = if code >= 0xf000 {
                        code & 0x1fff
                    } else {
                        code >> 4
                    };
                    let mut huff = u32::from(self.ac(ac + code as usize));
                    if huff == 0 {
                        return true;
                    }
                    bit_off += huff >> 8;
                    huff &= 0xff;
                    if huff == 0xf0 {
                        // Sixteen zeros.
                        index += 16;
                        continue;
                    }
                    if bit_off > 15 {
                        buf += 2;
                        bit_off -= 16;
                        bits = (bits << 16) | u32::from(self.be16(buf + 2));
                    }
                    if huff.trailing_zeros() >= 4 {
                        // End of band: its run count is not used.
                        bit_off += huff >> 4;
                        break;
                    }
                    index += (huff >> 4) as i32;
                    huff &= 0xf;
                    let mut code = bits.wrapping_shl(bit_off) >> (32 - huff);
                    if code & (0x8000_0000 >> (16 + huff)) == 0 {
                        code = code.wrapping_sub(0xffff_ffff >> (16 + huff));
                    }
                    bit_off += huff;
                    let code = code.wrapping_shl(u32::from(self.approx_low));
                    self.set_coeff(block + index, code as i16);
                    index += 1;
                }
            }
        }
        self.bit_buf = buf;
        self.vlc_off = buf as i32;
        self.bit_off = bit_off;
        self.bits = bits;
        false
    }

    /// JPEGDEC's integer IDCT of block `mcu` with quantization table `table`, leaving the 8x8
    /// pixels (2x2 at 1/4 size) as bytes at the start of the block. The passes run on a copy of
    /// the block: JPEGDEC's row pass wrote each row's bytes over rows it had already read.
    #[allow(
        clippy::too_many_lines,
        clippy::many_single_char_names,
        reason = "JPEGDEC's JPEGIDCT, its names and all, to read against it"
    )]
    fn idct(&mut self, mcu: i32, table: usize, failed: bool) {
        let out = mcu_at(mcu);
        let base = table * DCTSIZE;
        let raw = |jpeg: &Self, i: usize| W(i32::from(jpeg.coeff(mcu + i as i32)));
        let qq = |jpeg: &Self, i: usize| W(jpeg.quant_at(base + i));
        if self.options & scale::QUARTER != 0 {
            let tmp4 = raw(self, 0) * qq(self, 0);
            let tmp5 = raw(self, 8) * qq(self, 8);
            let tmp0 = tmp4 + tmp5;
            let tmp2 = tmp4 - tmp5;
            let tmp4 = raw(self, 1) * qq(self, 1);
            let tmp5 = raw(self, 9) * qq(self, 9);
            let tmp1 = tmp4 + tmp5;
            let tmp3 = tmp4 - tmp5;
            let bytes = [
                range(tmp0 + tmp1),
                range(tmp0 - tmp1),
                range(tmp2 + tmp3),
                range(tmp2 - tmp3),
            ];
            if let Some(slot) = self.mcus.get_mut(out..out + 4) {
                slot.copy_from_slice(&bytes);
            }
            return;
        }
        // Columns first, only those with data (column 0 always), into a copy as i16. A column
        // without data is all zeros, as `decode_mcu` cleared the block, unless its decode failed
        // part way: then the flags are the last block's and the copy starts from the block.
        let mut c = [0i16; DCTSIZE];
        if failed {
            for (i, v) in c.iter_mut().enumerate() {
                *v = self.coeff(mcu + i as i32);
            }
        }
        let mut flags = self.mcu_flags | 1;
        let mut col = 0;
        while col < 8 && flags != 0 {
            if flags & (1 << col) != 0 {
                flags &= !(1 << col);
                let (tmp0, tmp1, tmp2, tmp3, tmp4, tmp5, tmp6, tmp7);
                if flags & 0x2000 == 0 {
                    // Only the first half of the column is populated.
                    let tmp10 = raw(self, col) * qq(self, col);
                    let t1 = raw(self, col + 16) * qq(self, col + 16);
                    let tmp12 = (t1 * W(106)) >> 8;
                    tmp0 = tmp10 + t1;
                    tmp3 = tmp10 - t1;
                    tmp1 = tmp10 + tmp12;
                    tmp2 = tmp10 - tmp12;
                    let t4 = raw(self, col + 8) * qq(self, col + 8);
                    let t5 = raw(self, col + 24);
                    if t5.0 != 0 {
                        let t5 = t5 * qq(self, col + 24);
                        tmp7 = t4 + t5;
                        let tmp11 = ((t4 - t5) * W(362)) >> 8;
                        let z5 = ((t4 - t5) * W(473)) >> 8;
                        let tmp12 = ((-t5 * W(-669)) >> 8) + z5;
                        tmp6 = tmp12 - tmp7;
                        tmp5 = tmp11 - tmp6;
                        let tmp10 = ((t4 * W(277)) >> 8) - z5;
                        tmp4 = tmp10 + tmp5;
                    } else {
                        tmp7 = t4;
                        tmp5 = (W(145) * t4) >> 8;
                        tmp6 = (W(217) * t4) >> 8;
                        tmp4 = (W(-51) * t4) >> 8;
                    }
                } else {
                    let t0 = raw(self, col) * qq(self, col);
                    let t2 = raw(self, col + 32);
                    let (tmp10, tmp11) = if t2.0 != 0 {
                        let t2 = t2 * qq(self, col + 32);
                        (t0 + t2, t0 - t2)
                    } else {
                        (t0, t0)
                    };
                    let t1 = raw(self, col + 16) * qq(self, col + 16);
                    let t3 = raw(self, col + 48);
                    let (tmp13, tmp12) = if t3.0 != 0 {
                        let t3 = t3 * qq(self, col + 48);
                        let tmp13 = t1 + t3;
                        (tmp13, (((t1 - t3) * W(362)) >> 8) - tmp13)
                    } else {
                        (t1, ((t1 * W(362)) >> 8) - t1)
                    };
                    tmp0 = tmp10 + tmp13;
                    tmp3 = tmp10 - tmp13;
                    tmp1 = tmp11 + tmp12;
                    tmp2 = tmp11 - tmp12;
                    let t5 = raw(self, col + 24) * qq(self, col + 24);
                    let t6 = raw(self, col + 40);
                    let (z13, z10) = if t6.0 != 0 {
                        let t6 = t6 * qq(self, col + 40);
                        (t6 + t5, t6 - t5)
                    } else {
                        (t5, -t5)
                    };
                    let t4 = raw(self, col + 8) * qq(self, col + 8);
                    let t7 = raw(self, col + 56);
                    let (z11, z12) = if t7.0 != 0 {
                        let t7 = t7 * qq(self, col + 56);
                        (t4 + t7, t4 - t7)
                    } else {
                        (t4, t4)
                    };
                    tmp7 = z11 + z13;
                    let tmp11 = ((z11 - z13) * W(362)) >> 8;
                    let z5 = ((z10 + z12) * W(473)) >> 8;
                    let tmp12 = ((z10 * W(-669)) >> 8) + z5;
                    tmp6 = tmp12 - tmp7;
                    tmp5 = tmp11 - tmp6;
                    let tmp10 = ((z12 * W(277)) >> 8) - z5;
                    tmp4 = tmp10 + tmp5;
                }
                let rows = [
                    tmp0 + tmp7,
                    tmp1 + tmp6,
                    tmp2 + tmp5,
                    tmp3 - tmp4,
                    tmp3 + tmp4,
                    tmp2 - tmp5,
                    tmp1 - tmp6,
                    tmp0 - tmp7,
                ];
                for (r, value) in rows.into_iter().enumerate() {
                    if let Some(slot) = c.get_mut(col + 8 * r) {
                        // Stored as JPEGDEC's short.
                        *slot = value.0 as i16;
                    }
                }
            }
            col += 1;
        }
        let s = |c: &[i16; DCTSIZE], i: usize| W(i32::from(c.get(i).copied().unwrap_or(0)));
        // Then all rows, to 8 bytes each over the block (the copy holds what they read).
        let flags = self.mcu_flags;
        let Some(out) = self.mcus.get_mut(out..out + DCTSIZE) else {
            return;
        };
        for (row, dest) in out.as_chunks_mut::<8>().0.iter_mut().enumerate() {
            let r = row * 8;
            let (tmp0, tmp1, tmp2, tmp3, tmp4, tmp5, tmp6, tmp7);
            if flags & 0xf0 == 0 {
                if flags & 0xfc == 0 {
                    // One or two columns.
                    let t = s(&c, r);
                    (tmp0, tmp1, tmp2, tmp3) = (t, t, t, t);
                    tmp7 = s(&c, r + 1);
                    tmp6 = (tmp7 * W(217)) >> 8;
                    tmp5 = (tmp7 * W(145)) >> 8;
                    tmp4 = -((tmp7 * W(51)) >> 8);
                } else {
                    let tmp10 = s(&c, r);
                    let tmp13 = s(&c, r + 2);
                    let tmp12 = (tmp13 * W(106)) >> 8;
                    tmp0 = tmp10 + tmp13;
                    tmp3 = tmp10 - tmp13;
                    tmp1 = tmp10 + tmp12;
                    tmp2 = tmp10 - tmp12;
                    let z13 = s(&c, r + 3);
                    let z11 = s(&c, r + 1);
                    tmp7 = z11 + z13;
                    let tmp11 = ((z11 - z13) * W(362)) >> 8;
                    let z5 = ((z11 - z13) * W(473)) >> 8;
                    let tmp10 = ((z11 * W(277)) >> 8) - z5;
                    let tmp12 = ((z13 * W(669)) >> 8) + z5;
                    tmp6 = tmp12 - tmp7;
                    tmp5 = tmp11 - tmp6;
                    tmp4 = tmp10 + tmp5;
                }
            } else {
                let tmp10 = s(&c, r) + s(&c, r + 4);
                let tmp11 = s(&c, r) - s(&c, r + 4);
                let tmp13 = s(&c, r + 2) + s(&c, r + 6);
                let tmp12 = (((s(&c, r + 2) - s(&c, r + 6)) * W(362)) >> 8) - tmp13;
                tmp0 = tmp10 + tmp13;
                tmp3 = tmp10 - tmp13;
                tmp1 = tmp11 + tmp12;
                tmp2 = tmp11 - tmp12;
                let z13 = s(&c, r + 5) + s(&c, r + 3);
                let z10 = s(&c, r + 5) - s(&c, r + 3);
                let z11 = s(&c, r + 1) + s(&c, r + 7);
                let z12 = s(&c, r + 1) - s(&c, r + 7);
                tmp7 = z11 + z13;
                let tmp11 = ((z11 - z13) * W(362)) >> 8;
                let z5 = ((z10 + z12) * W(473)) >> 8;
                let tmp10 = ((z12 * W(277)) >> 8) - z5;
                let tmp12 = ((z10 * W(-669)) >> 8) + z5;
                tmp6 = tmp12 - tmp7;
                tmp5 = tmp11 - tmp6;
                tmp4 = tmp10 + tmp5;
            }
            let values = [
                tmp0 + tmp7,
                tmp1 + tmp6,
                tmp2 + tmp5,
                tmp3 - tmp4,
                tmp3 + tmp4,
                tmp2 - tmp5,
                tmp1 - tmp6,
                tmp0 - tmp7,
            ];
            for (d, value) in dest.iter_mut().zip(values) {
                *d = range(value);
            }
        }
    }

    /// `JPEGPutMCU8BitGray`: the luma of the MCU, to the pixel buffer at `x` in rows `pitch`
    /// bytes apart (at 1/2 size averaged, at 1/4 and 1/8 as decoded).
    #[allow(
        clippy::too_many_lines,
        reason = "JPEGDEC's JPEGPutMCU8BitGray, a case per layout and scale"
    )]
    fn put_mcu_gray(&mut self, x: i32, pitch: i32) {
        // The four luma blocks' bytes.
        let m = self.mcus.get(MCU_BASE..).unwrap_or_default();
        let get = |k: usize| m.get(k).copied().unwrap_or(0);
        let row = |k: usize, n: usize| m.get(k..k + n).unwrap_or_default();
        let average = |k: usize| {
            ((u32::from(get(k))
                + u32::from(get(k + 1))
                + u32::from(get(k + 8))
                + u32::from(get(k + 9))
                + 2)
                >> 2) as u8
        };
        let dest = x as isize;
        let pitch = pitch as isize;
        let pixels = &mut self.pixels;
        let half = self.options & scale::HALF != 0;
        let quarter = self.options & scale::QUARTER != 0;
        let eighth = self.options & scale::EIGHTH != 0;
        match self.sub_sample {
            ..=0x11 => {
                if half {
                    for i in 0..4 {
                        for j in 0..4 {
                            put(
                                pixels,
                                dest + i * pitch + j,
                                average((i * 16 + j * 2) as usize),
                            );
                        }
                    }
                    return;
                }
                if quarter {
                    for (d, s) in [(0, 0), (1, 1), (pitch, 2), (pitch + 1, 3)] {
                        put(pixels, dest + d, get(s));
                    }
                } else if eighth {
                    put(pixels, dest, get(0));
                } else {
                    for i in 0..8 {
                        put_row(pixels, dest + i * pitch, row(i as usize * 8, 8));
                    }
                }
            }
            0x21 => {
                if eighth {
                    put(pixels, dest, get(0));
                    put(pixels, dest + 1, get(128));
                } else if half {
                    for i in 0..4 {
                        for j in 0..4 {
                            let k = (i * 16 + j * 2) as usize;
                            put(pixels, dest + i * pitch + j, average(k));
                            put(pixels, dest + i * pitch + j + 4, average(k + 128));
                        }
                    }
                } else if quarter {
                    for (d, s) in [(0, 0), (1, 1), (pitch, 2), (pitch + 1, 3)] {
                        put(pixels, dest + d, get(s));
                        put(pixels, dest + d + 2, get(s + 128));
                    }
                } else {
                    for i in 0..8 {
                        let k = i as usize * 8;
                        put_row(pixels, dest + i * pitch, row(k, 8));
                        put_row(pixels, dest + i * pitch + 8, row(k + 128, 8));
                    }
                }
            }
            0x12 => {
                if eighth {
                    put(pixels, dest, get(0));
                    put(pixels, dest + pitch, get(128));
                } else if half {
                    for i in 0..4 {
                        for j in 0..4 {
                            let k = (i * 16 + j * 2) as usize;
                            put(pixels, dest + i * pitch + j, average(k));
                            put(pixels, dest + (i + 4) * pitch + j, average(k + 128));
                        }
                    }
                } else if quarter {
                    for (d, s) in [(0, 0), (1, 1), (pitch, 2), (pitch + 1, 3)] {
                        put(pixels, dest + d, get(s));
                        put(pixels, dest + d + 2 * pitch, get(s + 128));
                    }
                } else {
                    for i in 0..8 {
                        let k = i as usize * 8;
                        put_row(pixels, dest + i * pitch, row(k, 8));
                        put_row(pixels, dest + (i + 8) * pitch, row(k + 128, 8));
                    }
                }
            }
            0x22 => {
                if eighth {
                    put(pixels, dest, get(0));
                    put(pixels, dest + 1, get(128));
                    put(pixels, dest + pitch, get(256));
                    put(pixels, dest + pitch + 1, get(384));
                } else if quarter {
                    for (d, s) in [(0, 0), (1, 1), (pitch, 2), (pitch + 1, 3)] {
                        let d = dest + d;
                        put(pixels, d, get(s));
                        put(pixels, d + 2, get(s + 128));
                        put(pixels, d + 2 * pitch, get(s + 256));
                        put(pixels, d + 2 * pitch + 2, get(s + 384));
                    }
                } else if half {
                    for i in 0..4 {
                        for j in 0..4 {
                            let k = (i * 16 + j * 2) as usize;
                            let d = dest + i * pitch + j;
                            put(pixels, d, average(k));
                            put(pixels, d + 4, average(k + 128));
                            put(pixels, d + 4 * pitch, average(k + 256));
                            put(pixels, d + 4 * pitch + 4, average(k + 384));
                        }
                    }
                } else {
                    // Full size, clipped at the end of the block (the firmware's JPEGDEC had no
                    // unaligned fast path).
                    let count = pitch - x as isize;
                    if count >= 16 {
                        // The whole MCU fits, as always from `decode` (pitch is whole MCUs).
                        for i in 0..8 {
                            let k = i as usize * 8;
                            let d = dest + i * pitch;
                            put_row(pixels, d, row(k, 8));
                            put_row(pixels, d + 8 * pitch, row(k + 256, 8));
                            put_row(pixels, d + 8, row(k + 128, 8));
                            put_row(pixels, d + 8 * pitch + 8, row(k + 384, 8));
                        }
                        return;
                    }
                    let left = count.clamp(0, 8) as usize;
                    let right = (count - 8).clamp(0, 8) as usize;
                    for i in 0..8 {
                        let k = i as usize * 8;
                        let d = dest + i * pitch;
                        put_row(pixels, d, row(k, left));
                        put_row(pixels, d + 8 * pitch, row(k + 256, left));
                        put_row(pixels, d + 8, row(k + 128, right));
                        put_row(pixels, d + 8 * pitch + 8, row(k + 384, right));
                    }
                }
            }
            // Not reached: `decode` refuses other layouts.
            _ => {}
        }
    }
}

/// Writes a pixel at `at`; past the pixel buffer (JPEGDEC's undefined writes), nothing.
#[inline(always)]
fn put(pixels: &mut [u8], at: isize, value: u8) {
    if let Ok(at) = usize::try_from(at)
        && let Some(slot) = pixels.get_mut(at)
    {
        *slot = value;
    }
}

/// Writes the pixels of `row` from `at`, as `put` each of them.
#[inline(always)]
fn put_row(pixels: &mut [u8], at: isize, row: &[u8]) {
    if let Ok(start) = usize::try_from(at)
        && let Some(slot) = pixels.get_mut(start..start + row.len())
    {
        slot.copy_from_slice(row);
        return;
    }
    for (j, &value) in row.iter().enumerate() {
        put(pixels, at.wrapping_add(j as isize), value);
    }
}

/// The byte offset of MCU index `index` (in i16): `MCU_SKIP`'s -8 is the area's start.
#[inline(always)]
fn mcu_at(index: i32) -> usize {
    usize::try_from(index.wrapping_mul(2).wrapping_add(MCU_BASE as i32)).unwrap_or(usize::MAX)
}
