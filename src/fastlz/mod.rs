//! FastLZ (Z1) block decompressor used by Norton Ghost.
//!
//! See `docs/FORMAT.md` and `docs/FORMAT_OLD.md` for the wire format.
//!
//! The decompressor is a faithful port of the Python implementation in
//! `history_recovery.ghost_image.fastlz_decompress` (Norton Ghost 11.5.1
//! record layout, Z1 block framing).

use crate::error::{Error, Result};

/// Size of the FastLZ hash table (4096 entries).
pub const FASTLZ_HASH_SIZE: usize = 4096;

/// Sentinel byte sequence used to fill unreachable match positions.
pub const FASTLZ_SENTINEL: &[u8; 18] = b"123456789012345678";

const HASH_MULT: u32 = 24_993;

/// Compute the FastLZ hash slot for a 3-byte sequence.
#[inline]
fn fastlz_hash(b0: u32, b1: u32, b2: u32) -> usize {
    let v = b2 ^ (16 * (b1 ^ (16 * b0)));
    let neg = HASH_MULT.wrapping_mul(v).wrapping_neg();
    ((neg >> 4) & 0xFFF) as usize
}

/// Decompress one FastLZ (Z1) block.
///
/// `data` is the full block payload (including the 4-byte literal length prefix
/// embedded in the stream). `comp_len` is the number of valid bytes in `data`.
///
/// Block layout:
/// - byte 0 == 1 → uncompressed; bytes 1..4 are a 24-bit LE length, then
///   the literal payload follows at offset 4.
/// - byte 0 != 1 → compressed; bytes 0..4 are a literal-run control prefix,
///   then a stream of 16-bit control words and literals / back-references.
///
/// Output is capped at [`MAX_BLOCK_DECOMPRESSED`] bytes to defend against
/// decompression bombs.
pub fn decompress(data: &[u8], comp_len: usize) -> Result<Vec<u8>> {
    if comp_len == 0 || data.len() < comp_len {
        return Err(Error::fastlz(
            0,
            format!(
                "truncated compressed block (len={}, have {})",
                comp_len,
                data.len()
            ),
        ));
    }

    if data[0] == 1 {
        let n = comp_len
            .checked_sub(4)
            .ok_or_else(|| Error::fastlz(0, "corrupt uncompressed block"))?;
        if comp_len < 4 + n {
            return Err(Error::fastlz(0, "truncated uncompressed block"));
        }
        if n > MAX_BLOCK_DECOMPRESSED {
            return Err(Error::fastlz(
                0,
                format!(
                    "uncompressed block length {} exceeds max {}",
                    n, MAX_BLOCK_DECOMPRESSED
                ),
            ));
        }
        return Ok(data[4..4 + n].to_vec());
    }

    let mut hash_table = [usize::MAX; FASTLZ_HASH_SIZE];
    let mut out = Vec::with_capacity(comp_len.min(MAX_BLOCK_DECOMPRESSED));
    let mut src = 4usize;
    let src_end = comp_len;
    let mut control: u32 = 1;
    let mut literal_run: u32 = 0;
    let mut prev_literal_run: u32 = 0;

    while src < src_end {
        if control == 1 {
            if src + 1 >= src_end {
                break;
            }
            control = u32::from(data[src]) | (u32::from(data[src + 1]) << 8) | 0x1_0000;
            src += 2;
        }

        let near_end = src_end.saturating_sub(32) < src;
        let token_count = if near_end { 1 } else { 16 };

        for _ in 0..token_count {
            if src >= src_end {
                break;
            }

            if control & 1 != 0 {
                if src + 1 >= src_end {
                    control = 1;
                    src = src_end;
                    break;
                }

                let b0 = data[src];
                let b1 = data[src + 1];
                let hash_idx = usize::from(b1) | ((usize::from(b0 & 0xF0)) << 4);
                let extra_len = usize::from(b0 & 0x0F);
                let match_pos = hash_table[hash_idx];
                let match_start = out.len();
                let total_copy = 3 + extra_len;

                for j in 0..total_copy {
                    if out.len() >= MAX_BLOCK_DECOMPRESSED {
                        return Err(Error::fastlz(
                            0,
                            format!("decompressed output exceeds max {}", MAX_BLOCK_DECOMPRESSED),
                        ));
                    }
                    if match_pos == usize::MAX {
                        out.push(FASTLZ_SENTINEL.get(j).copied().unwrap_or(0));
                    } else {
                        let src_idx = match_pos + j;
                        out.push(if src_idx < out.len() { out[src_idx] } else { 0 });
                    }
                }

                src += 2;

                if literal_run > 0 {
                    let pos_signed = match_start as i64 - literal_run as i64;
                    if pos_signed >= 0 {
                        let pos = pos_signed as usize;
                        if pos + 2 < out.len() {
                            hash_table[fastlz_hash(
                                out[pos] as u32,
                                out[pos + 1] as u32,
                                out[pos + 2] as u32,
                            )] = pos;
                            if prev_literal_run == 2 && pos + 3 < out.len() {
                                hash_table[fastlz_hash(
                                    out[pos + 1] as u32,
                                    out[pos + 2] as u32,
                                    out[pos + 3] as u32,
                                )] = pos + 1;
                            }
                        }
                    }
                    literal_run = 0;
                    prev_literal_run = 0;
                }

                hash_table[hash_idx] = match_start;
            } else {
                if out.len() >= MAX_BLOCK_DECOMPRESSED {
                    return Err(Error::fastlz(
                        0,
                        format!("decompressed output exceeds max {}", MAX_BLOCK_DECOMPRESSED),
                    ));
                }
                literal_run += 1;
                out.push(data[src]);
                src += 1;
                prev_literal_run = literal_run;

                if literal_run == 3 {
                    let pos = out.len() - 3;
                    hash_table
                        [fastlz_hash(out[pos] as u32, out[pos + 1] as u32, out[pos + 2] as u32)] =
                        pos;
                    literal_run = 2;
                    prev_literal_run = 2;
                }
            }

            control >>= 1;
            if control == 1 {
                break;
            }
        }
    }

    Ok(out)
}

/// The fixed 32 KiB block size used by the partition payload layout.
pub const BLOCK_SIZE: usize = 32 * 1024;

/// Maximum length of a single compressed / uncompressed block payload
/// (excluding the 2-byte stored_len prefix that wraps every block on disk).
pub const MAX_BLOCK_STORED: usize = BLOCK_SIZE + 4 + 2;

/// Maximum decompressed output size for a single FastLZ block (defends
/// against decompression bombs — see `docs/SECURITY.md`).
///
/// A real Ghost block decompresses to at most `BLOCK_SIZE` (32 KiB). We
/// allow a 4× margin for malformed inputs and round to the next power of
/// two for cleanliness.
pub const MAX_BLOCK_DECOMPRESSED: usize = BLOCK_SIZE * 4;

#[cfg(test)]
mod tests {
    use super::*;

    /// Trusted reference block captured from a known-good encode of `b"ABC" * 240`.
    /// Source: history-recovery/tests/test_ghost_image.py::KNOWN_FASTLZ_BLOCK (93 bytes).
    const KNOWN_FASTLZ_BLOCK: [u8; 93] = [
        0x00, 0x00, 0x00, 0x00, 0xf8, 0xff, 0x41, 0x42, 0x43, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b,
        0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf,
        0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xff, 0xff, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b,
        0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf,
        0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xff, 0x07, 0xdf, 0x9b, 0xdf, 0x9b,
        0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf, 0x9b, 0xdf,
        0x9b, 0xdc, 0x9b,
    ];

    fn expected_abc_240() -> Vec<u8> {
        let mut v = Vec::with_capacity(720);
        for _ in 0..240 {
            v.extend_from_slice(b"ABC");
        }
        v
    }

    #[test]
    fn known_compressed_block_matches_trusted_encoder() {
        let got = decompress(&KNOWN_FASTLZ_BLOCK, KNOWN_FASTLZ_BLOCK.len()).unwrap();
        assert_eq!(got.len(), 720);
        assert_eq!(got, expected_abc_240());
    }

    #[test]
    fn uncompressed_block_escape() {
        // byte 0 == 1, bytes 1..4 = u24 LE length, then literal payload
        let mut block = vec![1u8, 11, 0, 0];
        block.extend_from_slice(b"hello world");
        let got = decompress(&block, block.len()).unwrap();
        assert_eq!(got, b"hello world");
    }

    #[test]
    fn empty_block_raises() {
        let err = decompress(b"", 0).unwrap_err();
        assert!(matches!(err, Error::FastLz { .. }));
    }

    #[test]
    fn truncated_block_raises() {
        let err = decompress(b"\x00", 5).unwrap_err();
        assert!(matches!(err, Error::FastLz { .. }));
    }

    #[test]
    fn uncompressed_with_too_small_block_raises() {
        // block claims to be uncompressed (byte[0]==1) but len < 4
        let err = decompress(b"\x01\x00", 2).unwrap_err();
        assert!(matches!(err, Error::FastLz { .. }));
    }

    #[test]
    fn hash_function_matches_python_port() {
        // From Python: v = b2 ^ (16 * (b1 ^ (16 * b0)))
        // hash = ((-24993 * v) & 0xFFFFFFFF) >> 4 & 0xFFF
        // Verified against the Python implementation directly: 3483 for "ABC".
        assert_eq!(fastlz_hash(b'A' as u32, b'B' as u32, b'C' as u32), 3483);
        assert_eq!(fastlz_hash(b'0' as u32, b'1' as u32, b'2' as u32), 3929);
        assert_eq!(fastlz_hash(0, 0, 0), 0);
    }
}
