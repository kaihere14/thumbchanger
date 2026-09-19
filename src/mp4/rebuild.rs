//! In-memory rewrite of the `moov` box: replace `udta/meta/ilst/covr`,
//! creating any missing ancestor, and recompute only the ancestors' sizes.
//! Every other byte of `moov` is copied verbatim.

use anyhow::Result;

use super::box_header::{BoxHeader, FourCc, with_header};
use super::finder::{COVR, HDLR, ILST, META, UDTA, child_prefix_len, container_children, moov_header};

/// What the rebuild did, for reporting.
#[derive(Debug, Default)]
pub struct RebuildStats {
    /// Number of existing `covr` boxes dropped.
    pub removed_covr: usize,
    /// Boxes that did not exist and were created.
    pub created: Vec<&'static str>,
}

/// Rebuild `moov` with `covr_box` (a complete, serialized `covr` box) as the
/// only cover item.
pub fn replace_cover(moov: &[u8], covr_box: &[u8]) -> Result<(Vec<u8>, RebuildStats)> {
    let hdr = moov_header(moov)?;
    let mut stats = RebuildStats::default();
    let out = rebuild(moov, &hdr, &[UDTA, META, ILST], covr_box, &mut stats)?;
    Ok((out, stats))
}

/// Rebuild container `hdr`, descending along `path` (the chain of child kinds
/// leading to `ilst`). An empty `path` means `hdr` is `ilst` itself.
fn rebuild(
    buf: &[u8],
    hdr: &BoxHeader,
    path: &[FourCc],
    covr_box: &[u8],
    stats: &mut RebuildStats,
) -> Result<Vec<u8>> {
    let payload = hdr.payload_offset() as usize;
    let prefix = child_prefix_len(buf, hdr);
    let kids = container_children(buf, hdr)?;

    let mut body = Vec::with_capacity(hdr.size as usize + covr_box.len());
    body.extend_from_slice(&buf[payload..payload + prefix]);

    match path.split_first() {
        None => {
            // This is `ilst`: keep every item except `covr`, append the new one.
            for k in &kids {
                if k.kind == COVR {
                    stats.removed_covr += 1;
                } else {
                    body.extend_from_slice(&buf[k.range()]);
                }
            }
            body.extend_from_slice(covr_box);
        }
        Some((next, rest)) => {
            if hdr.kind == META && !kids.iter().any(|k| k.kind == HDLR) {
                stats.created.push("hdlr");
                body.extend_from_slice(&hdlr_box());
            }
            let mut replaced = false;
            for k in &kids {
                if !replaced && k.kind == *next {
                    body.extend_from_slice(&rebuild(buf, k, rest, covr_box, stats)?);
                    replaced = true;
                } else {
                    body.extend_from_slice(&buf[k.range()]);
                }
            }
            if !replaced {
                body.extend_from_slice(&synthesize(*next, rest, covr_box, stats));
            }
        }
    }
    Ok(with_header(hdr.kind, hdr.header_len, &body))
}

/// Create a missing `udta` / `meta` / `ilst` chain around the cover.
fn synthesize(kind: FourCc, rest: &[FourCc], covr_box: &[u8], stats: &mut RebuildStats) -> Vec<u8> {
    stats.created.push(match kind {
        UDTA => "udta",
        META => "meta",
        _ => "ilst",
    });
    let body = match rest.split_first() {
        None => covr_box.to_vec(),
        Some((next, rest)) => {
            let mut b = Vec::new();
            if kind == META {
                b.extend_from_slice(&[0, 0, 0, 0]); // FullBox version + flags
                b.extend_from_slice(&hdlr_box());
            }
            b.extend_from_slice(&synthesize(*next, rest, covr_box, stats));
            b
        }
    };
    with_header(kind, 8, &body)
}

/// `hdlr` for iTunes metadata: handler `mdir`, reserved `appl`, empty name.
/// Matches what FFmpeg and iTunes write (33 bytes).
fn hdlr_box() -> Vec<u8> {
    let mut b = Vec::with_capacity(25);
    b.extend_from_slice(&[0, 0, 0, 0]); // version + flags
    b.extend_from_slice(&[0, 0, 0, 0]); // pre_defined
    b.extend_from_slice(b"mdir");
    b.extend_from_slice(b"appl");
    b.extend_from_slice(&[0; 8]); // reserved
    b.push(0); // empty name
    with_header(HDLR, 8, &b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mp4::finder::{MOOV, find_covr};

    fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        with_header(FourCc::new(kind), 8, body)
    }

    fn covr(payload: &[u8]) -> Vec<u8> {
        let mut d = vec![0, 0, 0, 13, 0, 0, 0, 0];
        d.extend_from_slice(payload);
        bx(b"covr", &bx(b"data", &d))
    }

    #[test]
    fn replaces_existing_covr_and_keeps_siblings() {
        let too = bx(b"\xa9too", &bx(b"data", b"\0\0\0\x01\0\0\0\0lavf"));
        let mut meta_body = vec![0, 0, 0, 0];
        meta_body.extend(hdlr_box());
        let mut ilst_body = too.clone();
        ilst_body.extend(covr(b"OLDOLDOLD"));
        ilst_body.extend(covr(b"SECOND"));
        meta_body.extend(bx(b"ilst", &ilst_body));
        let mvhd = bx(b"mvhd", &[7; 100]);
        let trak = bx(b"trak", &[9; 50]);
        let mut moov_body = mvhd.clone();
        moov_body.extend(&trak);
        moov_body.extend(bx(b"udta", &bx(b"meta", &meta_body)));
        let moov = bx(b"moov", &moov_body);

        let new = covr(b"NEW");
        let (out, stats) = replace_cover(&moov, &new).unwrap();
        assert_eq!(stats.removed_covr, 2);
        assert!(stats.created.is_empty());

        let hdr = moov_header(&out).unwrap();
        assert_eq!(hdr.kind, MOOV);
        assert_eq!(out.len(), moov.len() - covr(b"OLDOLDOLD").len() - covr(b"SECOND").len() + new.len());
        // mvhd + trak bytes untouched, at the same place.
        assert_eq!(&out[8..8 + mvhd.len() + trak.len()], &moov[8..8 + mvhd.len() + trak.len()]);
        let found = find_covr(&out).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(&out[found[0].range()], &new[..]);
        // ©too preserved before covr.
        assert!(out.windows(too.len()).any(|w| w == &too[..]));
    }

    #[test]
    fn creates_missing_chain() {
        let moov = bx(b"moov", &bx(b"mvhd", &[1; 20]));
        let new = covr(b"NEW");
        let (out, stats) = replace_cover(&moov, &new).unwrap();
        assert_eq!(stats.created, vec!["udta", "meta", "ilst"]);
        let found = find_covr(&out).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(&out[found[0].range()], &new[..]);
        // udta > meta > (vf + hdlr + ilst > covr)
        let expect = 8 + 8 + 4 + hdlr_box().len() + 8 + new.len();
        assert_eq!(out.len(), moov.len() + expect);
    }

    #[test]
    fn handles_quicktime_meta_without_version() {
        // meta whose payload starts directly with hdlr (no 4-byte prefix).
        let mut meta_body = hdlr_box();
        meta_body.extend(bx(b"ilst", &[]));
        let moov = bx(b"moov", &bx(b"udta", &bx(b"meta", &meta_body)));
        let new = covr(b"X");
        let (out, stats) = replace_cover(&moov, &new).unwrap();
        assert!(stats.created.is_empty());
        assert_eq!(find_covr(&out).unwrap().len(), 1);
        assert_eq!(out.len(), moov.len() + new.len());
    }
}
