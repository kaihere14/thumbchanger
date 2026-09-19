//! Cover artwork representation inside `covr/data`.

use anyhow::{Result, bail};

use super::box_header::{FourCc, with_header};
use super::finder::COVR;

/// Image formats we can embed. The discriminant is the iTunes `data` box
/// type indicator written in the 4 bytes following the `data` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Jpeg = 13,
    Png = 14,
}

impl ImageKind {
    /// Detect format from leading magic bytes.
    pub fn sniff(bytes: &[u8]) -> Result<ImageKind> {
        if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Ok(ImageKind::Jpeg)
        } else if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            Ok(ImageKind::Png)
        } else {
            bail!("unrecognized image magic bytes")
        }
    }

    /// Type indicator stored in the `data` box.
    pub fn data_type(self) -> u32 {
        self as u32
    }
}

/// Serialize a complete `covr` box holding one image:
///
/// ```text
/// covr
/// └── data
///     ├── u32 type indicator (13 JPEG / 14 PNG)
///     ├── u32 locale (0)
///     └── image bytes
/// ```
pub fn covr_box(kind: ImageKind, image: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(image.len() + 8);
    data.extend_from_slice(&kind.data_type().to_be_bytes());
    data.extend_from_slice(&0u32.to_be_bytes());
    data.extend_from_slice(image);
    with_header(COVR, 8, &with_header(FourCc::new(b"data"), 8, &data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_jpeg_and_png() {
        assert_eq!(ImageKind::sniff(&[0xFF, 0xD8, 0xFF, 0xE0]).unwrap(), ImageKind::Jpeg);
        assert_eq!(
            ImageKind::sniff(b"\x89PNG\r\n\x1a\nxxxx").unwrap(),
            ImageKind::Png
        );
        assert!(ImageKind::sniff(b"GIF89a").is_err());
        assert!(ImageKind::sniff(b"").is_err());
    }

    #[test]
    fn data_type_codes_match_itunes() {
        assert_eq!(ImageKind::Jpeg.data_type(), 13);
        assert_eq!(ImageKind::Png.data_type(), 14);
    }

    #[test]
    fn covr_box_layout_matches_ffmpeg() {
        // FFmpeg wrote covr size=272 / data size=264 for a 248-byte JPEG.
        let b = covr_box(ImageKind::Jpeg, &[0xAB; 248]);
        assert_eq!(b.len(), 272);
        assert_eq!(&b[0..8], &[0, 0, 1, 16, b'c', b'o', b'v', b'r']);
        assert_eq!(&b[8..16], &[0, 0, 1, 8, b'd', b'a', b't', b'a']);
        assert_eq!(&b[16..24], &[0, 0, 0, 13, 0, 0, 0, 0]);
        assert_eq!(b[24], 0xAB);
    }
}
