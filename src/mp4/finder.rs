//! Locate boxes inside an in-memory `moov` buffer. Only the paths we care
//! about are walked; everything else stays opaque.
//!
//! Offsets in the returned headers are relative to the start of the buffer
//! (i.e. the first byte of the `moov` header is offset 0).

use anyhow::{Result, bail};

use super::box_header::{BoxHeader, FourCc, children};

pub const MOOV: FourCc = FourCc::new(b"moov");
pub const MVEX: FourCc = FourCc::new(b"mvex");
pub const TRAK: FourCc = FourCc::new(b"trak");
pub const MDIA: FourCc = FourCc::new(b"mdia");
pub const MINF: FourCc = FourCc::new(b"minf");
pub const STBL: FourCc = FourCc::new(b"stbl");
pub const STCO: FourCc = FourCc::new(b"stco");
pub const CO64: FourCc = FourCc::new(b"co64");
pub const UDTA: FourCc = FourCc::new(b"udta");
pub const META: FourCc = FourCc::new(b"meta");
pub const HDLR: FourCc = FourCc::new(b"hdlr");
pub const ILST: FourCc = FourCc::new(b"ilst");
pub const COVR: FourCc = FourCc::new(b"covr");
pub const FREE: FourCc = FourCc::new(b"free");
pub const SKIP: FourCc = FourCc::new(b"skip");

/// Length of the fixed fields between a box header and its child boxes.
///
/// `meta` is a FullBox (4 bytes version+flags) in ISO MP4, but QuickTime
/// `.mov` files write it as a plain box. Sniff: a child box header is
/// `size(4) + type(4)`, so if the type `hdlr` appears 4 bytes into the
/// payload the child starts immediately and there is no version field.
pub fn child_prefix_len(buf: &[u8], hdr: &BoxHeader) -> usize {
    if hdr.kind != META {
        return 0;
    }
    let p = hdr.payload_offset() as usize;
    match buf.get(p + 4..p + 8) {
        Some(b"hdlr") => 0,
        _ => 4,
    }
}

/// Child boxes of a container, skipping any FullBox prefix.
pub fn container_children(buf: &[u8], hdr: &BoxHeader) -> Result<Vec<BoxHeader>> {
    let start = hdr.payload_offset() as usize + child_prefix_len(buf, hdr);
    children(buf, start, hdr.end() as usize)
}

/// Parse the `moov` header at the start of `buf`.
pub fn moov_header(buf: &[u8]) -> Result<BoxHeader> {
    match BoxHeader::parse(buf, 0, buf.len())? {
        Some(h) if h.kind == MOOV && h.end() as usize == buf.len() => Ok(h),
        Some(h) => bail!("expected a moov box spanning the buffer, found '{}' size {}", h.kind, h.size),
        None => bail!("empty moov buffer"),
    }
}

/// A `stco` / `co64` table: where its entries live and how many there are.
#[derive(Debug, Clone, Copy)]
pub struct ChunkTable {
    pub kind: FourCc,
    /// Buffer offset of the first entry.
    pub entries_offset: usize,
    pub count: usize,
}

impl ChunkTable {
    pub fn entry_len(&self) -> usize {
        if self.kind == CO64 { 8 } else { 4 }
    }

    /// Read entry `i` as an absolute file offset.
    pub fn get(&self, buf: &[u8], i: usize) -> u64 {
        let p = self.entries_offset + i * self.entry_len();
        if self.kind == CO64 {
            u64::from_be_bytes(buf[p..p + 8].try_into().unwrap())
        } else {
            u64::from(u32::from_be_bytes(buf[p..p + 4].try_into().unwrap()))
        }
    }

    /// Write entry `i`. Fails if a `stco` entry would overflow 32 bits.
    pub fn set(&self, buf: &mut [u8], i: usize, v: u64) -> Result<()> {
        let p = self.entries_offset + i * self.entry_len();
        if self.kind == CO64 {
            buf[p..p + 8].copy_from_slice(&v.to_be_bytes());
        } else {
            let v32 = u32::try_from(v).map_err(|_| {
                anyhow::anyhow!(
                    "chunk offset {v} no longer fits in a 32-bit stco table; \
                     stco -> co64 conversion is not supported"
                )
            })?;
            buf[p..p + 4].copy_from_slice(&v32.to_be_bytes());
        }
        Ok(())
    }
}

/// All chunk offset tables under `moov/trak/mdia/minf/stbl`.
pub fn chunk_tables(buf: &[u8]) -> Result<Vec<ChunkTable>> {
    let moov = moov_header(buf)?;
    let mut out = Vec::new();
    for trak in container_children(buf, &moov)?.iter().filter(|b| b.kind == TRAK) {
        for mdia in container_children(buf, trak)?.iter().filter(|b| b.kind == MDIA) {
            for minf in container_children(buf, mdia)?.iter().filter(|b| b.kind == MINF) {
                for stbl in container_children(buf, minf)?.iter().filter(|b| b.kind == STBL) {
                    for t in container_children(buf, stbl)? {
                        if t.kind != STCO && t.kind != CO64 {
                            continue;
                        }
                        let p = t.payload_offset() as usize;
                        let end = t.end() as usize;
                        if end - p < 8 {
                            bail!("truncated {} box", t.kind);
                        }
                        let count = u32::from_be_bytes(buf[p + 4..p + 8].try_into().unwrap()) as usize;
                        let table = ChunkTable { kind: t.kind, entries_offset: p + 8, count };
                        if p + 8 + count * table.entry_len() > end {
                            bail!("{} box declares {count} entries but is too small", t.kind);
                        }
                        out.push(table);
                    }
                }
            }
        }
    }
    Ok(out)
}

/// True if `moov` contains `mvex` (fragmented MP4: samples live in `moof`
/// fragments whose offsets we do not track).
pub fn is_fragmented(buf: &[u8]) -> Result<bool> {
    let moov = moov_header(buf)?;
    Ok(container_children(buf, &moov)?.iter().any(|b| b.kind == MVEX))
}

/// All `covr` boxes under `moov/udta/meta/ilst`.
pub fn find_covr(buf: &[u8]) -> Result<Vec<BoxHeader>> {
    let moov = moov_header(buf)?;
    let mut out = Vec::new();
    for udta in container_children(buf, &moov)?.iter().filter(|b| b.kind == UDTA) {
        for meta in container_children(buf, udta)?.iter().filter(|b| b.kind == META) {
            for ilst in container_children(buf, meta)?.iter().filter(|b| b.kind == ILST) {
                out.extend(container_children(buf, ilst)?.into_iter().filter(|b| b.kind == COVR));
            }
        }
    }
    Ok(out)
}
