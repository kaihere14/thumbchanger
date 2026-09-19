//! ISO-BMFF box header: 32-bit size + 4CC, with the 64-bit `largesize` and
//! `size == 0` (extends to end of file) variants.

use std::io::{self, Read};

use anyhow::{Result, bail};

/// Four-character box type.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FourCc(pub [u8; 4]);

impl FourCc {
    pub const fn new(s: &[u8; 4]) -> Self {
        FourCc(*s)
    }
}

impl std::fmt::Debug for FourCc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.0))
    }
}

impl std::fmt::Display for FourCc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.0))
    }
}

/// A parsed box header plus where it sits in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxHeader {
    pub kind: FourCc,
    /// Absolute offset of the first byte of the size field.
    pub offset: u64,
    /// Total box size including header. Resolved for the `size == 0` case.
    pub size: u64,
    /// Header length: 8, or 16 when `largesize` is used.
    pub header_len: u8,
}

impl BoxHeader {
    /// Absolute offset of the first payload byte.
    pub fn payload_offset(&self) -> u64 {
        self.offset + u64::from(self.header_len)
    }

    /// Absolute offset one past the last byte of the box.
    pub fn end(&self) -> u64 {
        self.offset + self.size
    }

    /// Read a header from `r`, which must be positioned at `offset`.
    /// `limit` is the end of the enclosing region (parent box end or file
    /// length); used to resolve `size == 0` and to reject overruns.
    pub fn read<R: Read>(r: &mut R, offset: u64, limit: u64) -> Result<Option<BoxHeader>> {
        let mut hdr = [0u8; 8];
        match r.read_exact(&mut hdr) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        let size32 = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
        let kind = FourCc([hdr[4], hdr[5], hdr[6], hdr[7]]);

        let (size, header_len) = match size32 {
            0 => (limit.saturating_sub(offset), 8),
            1 => {
                let mut large = [0u8; 8];
                r.read_exact(&mut large)?;
                (u64::from_be_bytes(large), 16)
            }
            n => (u64::from(n), 8),
        };

        if size < u64::from(header_len) {
            bail!("box '{kind}' at offset {offset} has invalid size {size}");
        }
        if offset + size > limit {
            bail!(
                "box '{kind}' at offset {offset} (size {size}) extends past its parent (end {limit})"
            );
        }
        Ok(Some(BoxHeader { kind, offset, size, header_len }))
    }

    /// Parse a header from an in-memory buffer. `pos` and `limit` are
    /// indices into `buf`; the returned `offset` is `pos`.
    pub fn parse(buf: &[u8], pos: usize, limit: usize) -> Result<Option<BoxHeader>> {
        let limit = limit.min(buf.len());
        if pos >= limit {
            return Ok(None);
        }
        Self::read(&mut io::Cursor::new(&buf[pos..limit]), pos as u64, limit as u64)
    }

    /// Byte range of this box within a buffer it was parsed from.
    pub fn range(&self) -> std::ops::Range<usize> {
        self.offset as usize..self.end() as usize
    }
}

/// Parse consecutive sibling boxes in `buf[start..end]`.
pub fn children(buf: &[u8], start: usize, end: usize) -> Result<Vec<BoxHeader>> {
    let mut out = Vec::new();
    let mut pos = start;
    while let Some(h) = BoxHeader::parse(buf, pos, end)? {
        pos = h.end() as usize;
        out.push(h);
    }
    Ok(out)
}

/// Serialize a box: header + body. Uses a 16-byte `largesize` header when
/// `header_len` is 16 or the size does not fit in 32 bits.
pub fn with_header(kind: FourCc, header_len: u8, body: &[u8]) -> Vec<u8> {
    let size32 = body.len() as u64 + 8;
    let mut out = Vec::with_capacity(body.len() + 16);
    if header_len == 16 || size32 > u64::from(u32::MAX) {
        out.extend_from_slice(&1u32.to_be_bytes());
        out.extend_from_slice(&kind.0);
        out.extend_from_slice(&(body.len() as u64 + 16).to_be_bytes());
    } else {
        out.extend_from_slice(&(size32 as u32).to_be_bytes());
        out.extend_from_slice(&kind.0);
    }
    out.extend_from_slice(body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reads_32bit_header() {
        let data = [0, 0, 0, 32, b'f', b't', b'y', b'p'];
        let h = BoxHeader::read(&mut Cursor::new(data), 0, 32).unwrap().unwrap();
        assert_eq!(h.kind, FourCc::new(b"ftyp"));
        assert_eq!(h.size, 32);
        assert_eq!(h.header_len, 8);
        assert_eq!(h.payload_offset(), 8);
    }

    #[test]
    fn reads_largesize_header() {
        let mut data = vec![0, 0, 0, 1, b'm', b'd', b'a', b't'];
        data.extend_from_slice(&0x1_0000_0010u64.to_be_bytes());
        let h = BoxHeader::read(&mut Cursor::new(data), 40, u64::MAX).unwrap().unwrap();
        assert_eq!(h.size, 0x1_0000_0010);
        assert_eq!(h.header_len, 16);
        assert_eq!(h.payload_offset(), 56);
    }

    #[test]
    fn size_zero_extends_to_limit() {
        let data = [0, 0, 0, 0, b'm', b'd', b'a', b't'];
        let h = BoxHeader::read(&mut Cursor::new(data), 100, 1000).unwrap().unwrap();
        assert_eq!(h.size, 900);
    }

    #[test]
    fn rejects_overrun_and_tiny_sizes() {
        let data = [0, 0, 0, 64, b'm', b'o', b'o', b'v'];
        assert!(BoxHeader::read(&mut Cursor::new(data), 0, 32).is_err());
        let data = [0, 0, 0, 4, b'm', b'o', b'o', b'v'];
        assert!(BoxHeader::read(&mut Cursor::new(data), 0, 32).is_err());
    }

    #[test]
    fn eof_yields_none() {
        assert!(BoxHeader::read(&mut Cursor::new([]), 0, 0).unwrap().is_none());
    }
}

#[cfg(test)]
mod serialize_tests {
    use super::*;

    #[test]
    fn with_header_roundtrips() {
        let b = with_header(FourCc::new(b"free"), 8, &[0; 4]);
        assert_eq!(b.len(), 12);
        let h = BoxHeader::parse(&b, 0, b.len()).unwrap().unwrap();
        assert_eq!(h.size, 12);
        assert_eq!(h.header_len, 8);

        let b = with_header(FourCc::new(b"mdat"), 16, &[0; 4]);
        assert_eq!(b.len(), 20);
        let h = BoxHeader::parse(&b, 0, b.len()).unwrap().unwrap();
        assert_eq!(h.size, 20);
        assert_eq!(h.header_len, 16);
    }

    #[test]
    fn children_parses_siblings() {
        let mut b = with_header(FourCc::new(b"aaaa"), 8, &[1, 2]);
        b.extend(with_header(FourCc::new(b"bbbb"), 8, &[]));
        let kids = children(&b, 0, b.len()).unwrap();
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[1].offset, 10);
        assert_eq!(kids[1].range(), 10..18);
    }
}
