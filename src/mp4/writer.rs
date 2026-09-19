//! Apply a plan by streaming segments into a new file. The input is only
//! ever read.
//!
//! Output is written to `<output>.part`, flushed and fsynced, then
//! **verified** ([`verify::check`]) before being renamed into place. A write
//! or verification failure removes the `.part` file, so the final path only
//! ever holds a checked result. Verification is not optional: it is the
//! postcondition of this function, for any caller.

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::planner::{Plan, Segment};
use super::verify::{self, Report};

/// Why `write` failed, so the CLI can tell a bad write from a failed check.
#[derive(Debug)]
pub enum WriteError {
    Write(anyhow::Error),
    Verify(anyhow::Error),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::Write(e) => write!(f, "write failed: {e:#}"),
            WriteError::Verify(e) => write!(f, "verification failed: {e:#}"),
        }
    }
}

impl std::error::Error for WriteError {}

/// Write, verify, then commit. On any error the `.part` file is removed and
/// nothing exists at `output`.
pub fn write(input: &Path, output: &Path, plan: &Plan) -> Result<Report, WriteError> {
    let part = part_path(output);
    let result = write_part(input, &part, plan)
        .map_err(WriteError::Write)
        .and_then(|()| verify::check(input, &part, plan).map_err(WriteError::Verify))
        .and_then(|report| {
            fs::rename(&part, output)
                .context("failed to move output into place")
                .map(|()| report)
                .map_err(WriteError::Write)
        });
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

fn part_path(output: &Path) -> PathBuf {
    let mut name = output.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".part");
    output.with_file_name(name)
}

fn write_part(input: &Path, part: &Path, plan: &Plan) -> Result<()> {
    let mut reader = BufReader::new(File::open(input)?);
    let out = File::create(part).with_context(|| format!("failed to create {}", part.display()))?;
    let mut w = BufWriter::with_capacity(1 << 20, out);

    for seg in &plan.segments {
        match seg {
            Segment::Bytes(b) => w.write_all(b)?,
            Segment::Copy { offset, len } => {
                reader.seek(SeekFrom::Start(*offset))?;
                let copied = io::copy(&mut (&mut reader).take(*len), &mut w)?;
                if copied != *len {
                    bail!("input truncated: expected {len} bytes at offset {offset}, got {copied}");
                }
            }
        }
    }
    w.flush()?;
    w.into_inner().map_err(|e| e.into_error())?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mp4::artwork::{ImageKind, covr_box};
    use crate::mp4::box_header::{FourCc, with_header};
    use crate::mp4::parser::scan_top_level;
    use crate::mp4::planner;

    fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        with_header(FourCc::new(kind), 8, body)
    }

    /// Smallest file the planner accepts: ftyp, moov with one chunk table
    /// pointing into mdat, mdat.
    fn tiny_mp4() -> Vec<u8> {
        let ftyp = bx(b"ftyp", b"isom\0\0\0\0isom");
        let mdat = bx(b"mdat", &[0xAB; 64]);
        let mut stco = vec![0, 0, 0, 0, 0, 0, 0, 1];
        let moov_len = 5 * 8 + (8 + 12); // moov/trak/mdia/minf/stbl headers + stco (vf, count, 1 entry)
        stco.extend((ftyp.len() as u32 + moov_len as u32 + 8).to_be_bytes());
        let moov = bx(b"moov", &bx(b"trak", &bx(b"mdia", &bx(b"minf", &bx(b"stbl", &bx(b"stco", &stco))))));
        assert_eq!(moov.len(), moov_len);
        [ftyp, moov, mdat].concat()
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("thumbchanger-writer-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        d.join(name)
    }

    #[test]
    fn verification_failure_leaves_no_output() {
        let input = tmp("in.mp4");
        let output = tmp("out.mp4");
        fs::write(&input, tiny_mp4()).unwrap();
        let top = scan_top_level(&input).unwrap();
        let covr = covr_box(ImageKind::Png, b"\x89PNG\r\n\x1a\nfake");

        // Sane plan commits.
        let plan = planner::plan(&input, &top, &covr).unwrap();
        write(&input, &output, &plan).unwrap();
        assert!(output.exists());
        fs::remove_file(&output).unwrap();

        // Corrupt the plan so the written file does not match the input:
        // drop the trailing mdat segment.
        let mut bad = planner::plan(&input, &top, &covr).unwrap();
        let last = bad.segments.pop().unwrap();
        if let Segment::Copy { len, .. } = last {
            bad.output_len -= len;
        }
        match write(&input, &output, &bad) {
            Err(WriteError::Verify(_)) => {}
            other => panic!("expected verification failure, got {other:?}"),
        }
        assert!(!output.exists(), "output must not be committed");
        assert!(!part_path(&output).exists(), ".part must be removed");
        let _ = fs::remove_dir_all(input.parent().unwrap());
    }
}
