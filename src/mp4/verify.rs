//! Post-write verification, run on the `.part` file before it is renamed
//! into place. Independent of the planner's bookkeeping:
//!
//! - every `mdat` payload in the output hashes identically to the input,
//! - the output `moov` parses and contains exactly one `covr` of the
//!   expected size,
//! - every chunk offset equals the input offset shifted by exactly what the
//!   plan says (0, or `delta` for offsets past the old `moov`),
//! - output length matches the plan.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, Result, bail};

use super::box_header::{BoxHeader, FourCc};
use super::finder::{self, MOOV};
use super::parser::scan_top_level;
use super::planner::{Plan, Strategy};

const MDAT: FourCc = FourCc::new(b"mdat");

/// What was checked, for reporting.
#[derive(Debug, Clone, Copy)]
pub struct Report {
    pub mdat_count: usize,
    pub mdat_bytes: u64,
    pub chunk_offsets_checked: usize,
}

pub fn check(input: &Path, output: &Path, plan: &Plan) -> Result<Report> {
    let in_top = scan_top_level(input)?;
    let out_top = scan_top_level(output)?;

    let out_len = File::open(output)?.metadata()?.len();
    if out_len != plan.output_len {
        bail!("output is {out_len} bytes, plan expected {}", plan.output_len);
    }

    // mdat payloads: same count, same size, same content.
    let in_mdat: Vec<_> = in_top.iter().filter(|b| b.kind == MDAT).collect();
    let out_mdat: Vec<_> = out_top.iter().filter(|b| b.kind == MDAT).collect();
    if in_mdat.len() != out_mdat.len() {
        bail!("mdat count changed: {} -> {}", in_mdat.len(), out_mdat.len());
    }
    let mut mdat_bytes = 0;
    let mut rin = BufReader::with_capacity(1 << 20, File::open(input)?);
    let mut rout = BufReader::with_capacity(1 << 20, File::open(output)?);
    for (a, b) in in_mdat.iter().zip(&out_mdat) {
        if a.size != b.size || a.header_len != b.header_len {
            bail!("mdat size changed: {} -> {}", a.size, b.size);
        }
        let len = a.size - u64::from(a.header_len);
        let ha = fnv1a(&mut rin, a.payload_offset(), len)?;
        let hb = fnv1a(&mut rout, b.payload_offset(), len)?;
        if ha != hb {
            bail!("mdat payload at input offset {} differs from output", a.offset);
        }
        mdat_bytes += len;
    }

    // moov: parse, covr present, chunk offsets consistent with plan.
    let in_moov = read_box(&mut rin, in_top.iter().find(|b| b.kind == MOOV).context("input has no moov")?)?;
    let out_moov = read_box(&mut rout, out_top.iter().find(|b| b.kind == MOOV).context("output has no moov")?)?;
    let checked = check_moov_pair(&in_moov, &out_moov, &MoovExpectation::from_plan(plan))?;

    Ok(Report { mdat_count: in_mdat.len(), mdat_bytes, chunk_offsets_checked: checked })
}

/// What the output `moov` must satisfy relative to the input `moov`.
#[derive(Debug, Clone, Copy)]
pub struct MoovExpectation {
    /// Serialized size of the single `covr` box expected in the output.
    pub covr_len: usize,
    /// Amount every chunk offset at or past `old_moov_end` must have moved.
    pub shift: i64,
    /// End offset of the input `moov` box.
    pub old_moov_end: u64,
}

impl MoovExpectation {
    pub fn from_plan(plan: &Plan) -> Self {
        MoovExpectation {
            covr_len: plan.covr_len,
            shift: if plan.strategy == Strategy::ShiftAndPatch { plan.delta } else { 0 },
            old_moov_end: plan.old_moov.end(),
        }
    }
}

/// IO-free half of [`check`]: compare two in-memory `moov` boxes. Returns
/// the number of chunk offsets checked.
pub fn check_moov_pair(in_moov: &[u8], out_moov: &[u8], want: &MoovExpectation) -> Result<usize> {
    let covr = finder::find_covr(out_moov)?;
    match covr.as_slice() {
        [c] if c.size as usize == want.covr_len => {}
        [c] => bail!("output covr is {} bytes, expected {}", c.size, want.covr_len),
        other => bail!("expected exactly one covr in output, found {}", other.len()),
    }

    let in_tables = finder::chunk_tables(in_moov)?;
    let out_tables = finder::chunk_tables(out_moov)?;
    if in_tables.len() != out_tables.len() {
        bail!("chunk table count changed");
    }
    let mut checked = 0;
    for (a, b) in in_tables.iter().zip(&out_tables) {
        if a.count != b.count || a.kind != b.kind {
            bail!("chunk table shape changed");
        }
        for i in 0..a.count {
            let x = a.get(in_moov, i);
            let y = b.get(out_moov, i);
            let expect = if x >= want.old_moov_end { (x as i64 + want.shift) as u64 } else { x };
            if y != expect {
                bail!("chunk offset entry {i} is {y}, expected {expect} (input {x})");
            }
            checked += 1;
        }
    }
    Ok(checked)
}

pub const FNV1A_SEED: u64 = 0xcbf2_9ce4_8422_2325;

/// Fold `bytes` into a running FNV-1a 64 hash.
pub fn fnv1a_update(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn read_box<R: Read + Seek>(r: &mut R, h: &BoxHeader) -> Result<Vec<u8>> {
    r.seek(SeekFrom::Start(h.offset))?;
    let mut buf = vec![0u8; h.size as usize];
    r.read_exact(&mut buf)?;
    if u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) == 0 {
        buf[0..4].copy_from_slice(&(h.size as u32).to_be_bytes());
    }
    Ok(buf)
}

/// Streaming FNV-1a 64 over `len` bytes starting at `offset`. Not
/// cryptographic; used only to compare input against output we just wrote.
fn fnv1a<R: Read + Seek>(r: &mut R, offset: u64, len: u64) -> Result<u64> {
    r.seek(SeekFrom::Start(offset))?;
    let mut h = FNV1A_SEED;
    let mut remaining = len;
    let mut buf = vec![0u8; 1 << 20];
    while remaining > 0 {
        let want = remaining.min(buf.len() as u64) as usize;
        let n = r.read(&mut buf[..want])?;
        if n == 0 {
            bail!("unexpected EOF while hashing");
        }
        h = fnv1a_update(h, &buf[..n]);
        remaining -= n as u64;
    }
    Ok(h)
}
