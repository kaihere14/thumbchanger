//! Decide what must change before anything is written.
//!
//! Output is a list of segments: byte ranges copied verbatim from the input,
//! or new bytes. The planner also decides how a change in `moov` size is
//! absorbed so that `mdat` bytes are never touched:
//!
//! 1. `moov` is the last box: nothing after it can move. Just write it.
//! 2. A `free`/`skip` box directly follows `moov`: resize it by the delta so
//!    every later box keeps its offset.
//! 3. `moov` shrank by >= 8 bytes: insert a new `free` box after it.
//! 4. Otherwise: shift everything after `moov` and patch every `stco`/`co64`
//!    entry that points past the old `moov` end.
//!
//! Refused: fragmented MP4 (`moof`, `sidx`, `mvex`), multiple `moov`.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, Result, bail};

use super::box_header::{BoxHeader, FourCc, with_header};
use super::finder::{self, FREE, MOOV, SKIP};
use super::rebuild::{self, RebuildStats};

/// Refuse to hold a `moov` larger than this in memory (sanity limit; real
/// files are KBs to tens of MBs).
const MAX_MOOV_BYTES: u64 = 1 << 30;

#[derive(Debug)]
pub enum Segment {
    /// Copy `len` bytes from `offset` in the input.
    Copy { offset: u64, len: u64 },
    /// Write these bytes.
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    /// `moov` size unchanged.
    SameSize,
    /// `moov` is the last box; nothing after it to move.
    MoovLast,
    /// Adjacent `free`/`skip` box resized to absorb the delta.
    ResizeFree,
    /// New `free` box inserted after `moov` to fill the gap.
    InsertFree,
    /// Boxes after `moov` shifted; chunk offsets patched.
    ShiftAndPatch,
}

#[derive(Debug)]
pub struct Plan {
    pub segments: Vec<Segment>,
    pub strategy: Strategy,
    /// `new moov size - old moov size`.
    pub delta: i64,
    /// Number of `stco`/`co64` entries rewritten.
    pub patched_offsets: usize,
    pub old_moov: BoxHeader,
    pub rebuild: RebuildStats,
    /// Total output length.
    pub output_len: u64,
    /// Size of the serialized `covr` box written.
    pub covr_len: usize,
}

/// Plan against a file on disk: reads `moov` from `input`, then
/// [`plan_moov`].
pub fn plan(input: &Path, top: &[BoxHeader], covr_box: &[u8]) -> Result<Plan> {
    validate_top_level(top)?;
    let old_moov = top.iter().find(|b| b.kind == MOOV).expect("validated");
    let moov_buf = read_moov(input, old_moov)?;
    plan_moov(top, moov_buf, covr_box)
}

/// Plan with the `moov` box already in memory (`moov_buf` must be exactly
/// the bytes of the `moov` box in `top`). IO-free, so it is shared by the
/// CLI and the WebAssembly build.
pub fn plan_moov(top: &[BoxHeader], mut moov_buf: Vec<u8>, covr_box: &[u8]) -> Result<Plan> {
    validate_top_level(top)?;
    let moov_idx = top.iter().position(|b| b.kind == MOOV).expect("validated");
    let old_moov = top[moov_idx];
    if moov_buf.len() as u64 != old_moov.size {
        bail!("moov buffer is {} bytes, header says {}", moov_buf.len(), old_moov.size);
    }
    normalize_size_field(&mut moov_buf, old_moov.size)?;

    if finder::is_fragmented(&moov_buf)? {
        bail!("fragmented MP4 (moov/mvex) is not supported");
    }

    let (mut new_moov, rebuild_stats) = rebuild::replace_cover(&moov_buf, covr_box)?;
    let delta = new_moov.len() as i64 - old_moov.size as i64;
    let next = top.get(moov_idx + 1).copied();

    let mut patched_offsets = 0;
    let strategy = if delta == 0 {
        Strategy::SameSize
    } else if next.is_none() {
        Strategy::MoovLast
    } else if let Some(f) = next.filter(|f| f.kind == FREE || f.kind == SKIP)
        && let Some(new_size) = resized_free(&f, delta)
    {
        let _ = new_size;
        Strategy::ResizeFree
    } else if delta <= -8 {
        Strategy::InsertFree
    } else {
        patched_offsets = patch_chunk_offsets(&mut new_moov, &old_moov, delta)?;
        Strategy::ShiftAndPatch
    };

    let mut segments = Vec::with_capacity(top.len() + 1);
    let mut output_len = 0u64;
    let push = |seg: Segment, len: u64, out: &mut Vec<Segment>, total: &mut u64| {
        *total += len;
        out.push(seg);
    };
    for (i, b) in top.iter().enumerate() {
        if i == moov_idx {
            let len = new_moov.len() as u64;
            push(Segment::Bytes(std::mem::take(&mut new_moov)), len, &mut segments, &mut output_len);
            if strategy == Strategy::InsertFree {
                let free = with_header(FREE, 8, &vec![0u8; (-delta - 8) as usize]);
                let len = free.len() as u64;
                push(Segment::Bytes(free), len, &mut segments, &mut output_len);
            }
        } else if i == moov_idx + 1 && strategy == Strategy::ResizeFree {
            let new_size = resized_free(b, delta).expect("checked above");
            let free = with_header(b.kind, 8, &vec![0u8; (new_size - 8) as usize]);
            push(Segment::Bytes(free), new_size, &mut segments, &mut output_len);
        } else {
            push(Segment::Copy { offset: b.offset, len: b.size }, b.size, &mut segments, &mut output_len);
        }
    }

    Ok(Plan {
        segments,
        strategy,
        delta,
        patched_offsets,
        old_moov,
        rebuild: rebuild_stats,
        output_len,
        covr_len: covr_box.len(),
    })
}

/// New size for a `free` box absorbing `delta`, if it stays a valid box
/// (>= 8 bytes, 32-bit size).
fn resized_free(free: &BoxHeader, delta: i64) -> Option<u64> {
    let new_size = free.size as i64 - delta;
    (new_size >= 8 && new_size <= i64::from(u32::MAX)).then_some(new_size as u64)
}

fn validate_top_level(top: &[BoxHeader]) -> Result<()> {
    let count = |k: &[u8; 4]| top.iter().filter(|b| b.kind == FourCc::new(k)).count();
    if count(b"moov") != 1 {
        bail!("expected exactly one moov box, found {}", count(b"moov"));
    }
    if count(b"moof") > 0 || count(b"sidx") > 0 {
        bail!("fragmented MP4 (moof/sidx) is not supported");
    }
    if count(b"mdat") == 0 {
        bail!("no mdat box found");
    }
    Ok(())
}

fn read_moov(input: &Path, moov: &BoxHeader) -> Result<Vec<u8>> {
    if moov.size > MAX_MOOV_BYTES {
        bail!("moov box is {} bytes; refusing to load more than {MAX_MOOV_BYTES}", moov.size);
    }
    let mut f = File::open(input).with_context(|| format!("failed to open {}", input.display()))?;
    f.seek(SeekFrom::Start(moov.offset))?;
    let mut buf = vec![0u8; moov.size as usize];
    f.read_exact(&mut buf).context("failed to read moov box")?;
    Ok(buf)
}

/// A `size == 0` moov (extends to EOF) is rewritten with an explicit size;
/// normalize the header so downstream parsing sees a self-consistent box.
fn normalize_size_field(buf: &mut [u8], size: u64) -> Result<()> {
    if buf.len() >= 4 && u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) == 0 {
        let size = u32::try_from(size).context("moov > 4 GiB with size==0 header")?;
        buf[0..4].copy_from_slice(&size.to_be_bytes());
    }
    Ok(())
}

/// Shift every chunk offset that pointed at or past the end of the old
/// `moov` by `delta`. Offsets before `moov` are untouched. Returns the
/// number of entries rewritten.
fn patch_chunk_offsets(moov: &mut [u8], old_moov: &BoxHeader, delta: i64) -> Result<usize> {
    let tables = finder::chunk_tables(moov)?;
    let old_end = old_moov.end();
    let mut patched = 0;
    for t in tables {
        for i in 0..t.count {
            let v = t.get(moov, i);
            if v >= old_end {
                let nv = (v as i64 + delta) as u64;
                t.set(moov, i, nv)?;
                patched += 1;
            } else if v >= old_moov.offset {
                bail!("chunk offset {v} points inside the moov box; refusing to relocate it");
            }
        }
    }
    Ok(patched)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mp4::box_header::with_header;

    fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        with_header(FourCc::new(kind), 8, body)
    }

    fn moov_with_tables(stco: &[u32], co64: &[u64]) -> Vec<u8> {
        let mut s = vec![0, 0, 0, 0];
        s.extend((stco.len() as u32).to_be_bytes());
        for v in stco {
            s.extend(v.to_be_bytes());
        }
        let mut c = vec![0, 0, 0, 0];
        c.extend((co64.len() as u32).to_be_bytes());
        for v in co64 {
            c.extend(v.to_be_bytes());
        }
        let mut stbl = bx(b"stco", &s);
        stbl.extend(bx(b"co64", &c));
        let trak = bx(b"trak", &bx(b"mdia", &bx(b"minf", &bx(b"stbl", &stbl))));
        bx(b"moov", &trak)
    }

    #[test]
    fn patches_only_offsets_past_old_moov() {
        let mut moov = moov_with_tables(&[10, 1000, 5000], &[20, 1000, 1 << 33]);
        let old = BoxHeader { kind: MOOV, offset: 100, size: 900, header_len: 8 };
        let n = patch_chunk_offsets(&mut moov, &old, 50).unwrap();
        assert_eq!(n, 4);
        let t = finder::chunk_tables(&moov).unwrap();
        assert_eq!([t[0].get(&moov, 0), t[0].get(&moov, 1), t[0].get(&moov, 2)], [10, 1050, 5050]);
        assert_eq!([t[1].get(&moov, 0), t[1].get(&moov, 1), t[1].get(&moov, 2)], [20, 1050, (1 << 33) + 50]);
    }

    #[test]
    fn rejects_offsets_inside_moov_and_stco_overflow() {
        let mut moov = moov_with_tables(&[500], &[]);
        let old = BoxHeader { kind: MOOV, offset: 100, size: 900, header_len: 8 };
        assert!(patch_chunk_offsets(&mut moov, &old, 8).is_err());

        let mut moov = moov_with_tables(&[u32::MAX - 4], &[]);
        let old = BoxHeader { kind: MOOV, offset: 0, size: 100, header_len: 8 };
        assert!(patch_chunk_offsets(&mut moov, &old, 8).is_err());
    }

    #[test]
    fn resized_free_bounds() {
        let f = BoxHeader { kind: FREE, offset: 0, size: 8, header_len: 8 };
        assert_eq!(resized_free(&f, 0), Some(8));
        assert_eq!(resized_free(&f, 1), None);
        assert_eq!(resized_free(&f, -100), Some(108));
        let f = BoxHeader { kind: FREE, offset: 0, size: 1000, header_len: 8 };
        assert_eq!(resized_free(&f, 992), Some(8));
        assert_eq!(resized_free(&f, 993), None);
    }
}
