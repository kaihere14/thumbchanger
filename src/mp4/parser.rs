//! Top-level box scan. Reads only headers; never loads payloads.

use std::fs::File;
use std::io::{BufReader, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, Result};

use super::box_header::BoxHeader;

/// Scan the top-level boxes of an MP4 file (`ftyp`, `moov`, `mdat`, ...).
pub fn scan_top_level(path: &Path) -> Result<Vec<BoxHeader>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let len = file.metadata()?.len();
    let mut r = BufReader::new(file);
    let mut boxes = Vec::new();
    let mut pos = 0u64;
    while pos < len {
        r.seek(SeekFrom::Start(pos))?;
        let Some(h) = BoxHeader::read(&mut r, pos, len)? else { break };
        pos = h.end();
        boxes.push(h);
    }
    Ok(boxes)
}
