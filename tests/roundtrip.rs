//! End-to-end tests against real MP4 files generated with FFmpeg.
//! Skipped (with a message) when `ffmpeg`/`ffprobe` are not on PATH.
//!
//! Independent of the tool's own verification: media packets are hashed with
//! FFmpeg's `framehash` muxer before and after, and cover presence is read
//! back with ffprobe.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_thumbchanger");

struct Fixtures {
    dir: PathBuf,
}

impl Fixtures {
    fn new(name: &str) -> Option<Self> {
        if Command::new("ffmpeg").arg("-version").output().is_err() {
            eprintln!("skipping {name}: ffmpeg not found");
            return None;
        }
        let dir = std::env::temp_dir().join(format!("thumbchanger-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let f = Fixtures { dir };
        f.ffmpeg(&["-f", "lavfi", "-i", "testsrc=size=160x120:rate=30", "-f", "lavfi", "-i", "sine=frequency=440",
            "-t", "2", "-c:v", "mpeg4", "-c:a", "aac", "base.mp4"]);
        f.ffmpeg(&["-f", "lavfi", "-i", "color=c=red:size=64x64", "-frames:v", "1", "small.jpg"]);
        f.ffmpeg(&["-f", "lavfi", "-i", "color=c=blue:size=64x64", "-frames:v", "1", "small.png"]);
        f.ffmpeg(&["-f", "lavfi", "-i", "testsrc=size=640x480", "-frames:v", "1", "big.jpg"]);
        Some(f)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn ffmpeg(&self, args: &[&str]) {
        let st = Command::new("ffmpeg").current_dir(&self.dir).args(["-loglevel", "error", "-y"]).args(args).status().unwrap();
        assert!(st.success(), "ffmpeg {args:?} failed");
    }

    /// Remux with an embedded cover, optionally faststart (moov before mdat).
    fn with_cover(&self, src: &str, cover: &str, faststart: bool, out: &str) {
        let mut args = vec!["-i", src, "-i", cover, "-map", "0", "-map", "1", "-c", "copy", "-disposition:v:1", "attached_pic"];
        if faststart {
            args.extend(["-movflags", "+faststart"]);
        }
        args.push(out);
        self.ffmpeg(&args);
    }

    /// Hash of all compressed packets of stream `spec` (e.g. "0:v:0").
    fn packet_hash(&self, file: &str, spec: &str) -> String {
        let out = Command::new("ffmpeg").current_dir(&self.dir)
            .args(["-loglevel", "error", "-i", file, "-map", spec, "-c", "copy", "-f", "framehash", "-hash", "sha256", "-"])
            .output().unwrap();
        assert!(out.status.success(), "framehash failed for {file}");
        String::from_utf8(out.stdout).unwrap().lines().filter(|l| !l.starts_with('#')).collect::<Vec<_>>().join("\n")
    }

    /// (codec, width, height) of the attached picture as seen by ffprobe.
    fn cover_info(&self, file: &str) -> String {
        let out = Command::new("ffprobe").current_dir(&self.dir)
            .args(["-loglevel", "error", "-select_streams", "v:1", "-show_entries", "stream=codec_name,width,height", "-of", "csv=p=0", file])
            .output().unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    fn decodes_cleanly(&self, file: &str) -> bool {
        let out = Command::new("ffmpeg").current_dir(&self.dir)
            .args(["-loglevel", "error", "-i", file, "-f", "null", "-"]).output().unwrap();
        out.status.success() && out.stderr.is_empty()
    }

    fn run(&self, input: &str, thumb: &str, output: &str) -> Result<String, String> {
        let out = Command::new(BIN).current_dir(&self.dir).args([input, "-t", thumb, "-o", output]).output().unwrap();
        let stdout = String::from_utf8(out.stdout).unwrap();
        let stderr = String::from_utf8(out.stderr).unwrap();
        if out.status.success() { Ok(stdout) } else { Err(stderr) }
    }

    /// Run, then assert packets identical, cover as expected, plays back.
    fn check(&self, input: &str, thumb: &str, output: &str, expect_cover: &str, expect_layout: &str) {
        let stdout = self.run(input, thumb, output).unwrap_or_else(|e| panic!("{input} + {thumb}: {e}"));
        assert!(stdout.contains(expect_layout), "{input}: expected layout {expect_layout} in:\n{stdout}");
        assert!(stdout.contains("✓ Verification passed"), "{input}: verification missing:\n{stdout}");
        assert_eq!(self.packet_hash(input, "0:v:0"), self.packet_hash(output, "0:v:0"), "{input}: video packets changed");
        assert_eq!(self.packet_hash(input, "0:a:0"), self.packet_hash(output, "0:a:0"), "{input}: audio packets changed");
        assert_eq!(self.cover_info(output), expect_cover, "{input}: cover mismatch");
        assert!(self.decodes_cleanly(output), "{input}: output does not decode cleanly");
    }
}

impl Drop for Fixtures {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Top-level (type, offset, size) triples.
fn top_level(path: &Path) -> Vec<([u8; 4], usize, usize)> {
    let b = fs::read(path).unwrap();
    let mut v = Vec::new();
    let mut p = 0;
    while p < b.len() {
        let s = u32::from_be_bytes(b[p..p + 4].try_into().unwrap()) as usize;
        v.push((b[p + 4..p + 8].try_into().unwrap(), p, s));
        p += s;
    }
    v
}

#[test]
fn moov_last_add_cover() {
    let Some(f) = Fixtures::new("moovlast") else { return };
    // base.mp4 already carries udta/meta/ilst (FFmpeg's ©too tag); only covr is new.
    let stdout = f.run("base.mp4", "small.jpg", "out.mp4").unwrap();
    assert!(!stdout.contains("created"), "{stdout}");
    f.check("base.mp4", "small.jpg", "out.mp4", "mjpeg,64,64", "moov is last box");

    // Without udta the whole chain must be synthesized. FFmpeg always writes
    // udta/meta, so cut it out by hand (moov is last: no offsets to patch).
    let src = fs::read(f.path("base.mp4")).unwrap();
    let top = top_level(&f.path("base.mp4"));
    let (_, moov_off, moov_len) = *top.iter().find(|x| &x.0 == b"moov").unwrap();
    assert_eq!(moov_off + moov_len, src.len(), "expected moov to be the last box");
    let mut p = moov_off + 8;
    let mut bare = src.clone();
    while p < moov_off + moov_len {
        let s = u32::from_be_bytes(src[p..p + 4].try_into().unwrap()) as usize;
        if &src[p + 4..p + 8] == b"udta" {
            bare.drain(p..p + s);
            let new_len = (moov_len - s) as u32;
            bare[moov_off..moov_off + 4].copy_from_slice(&new_len.to_be_bytes());
            break;
        }
        p += s;
    }
    assert!(bare.len() < src.len(), "no udta found to strip");
    fs::write(f.path("bare.mp4"), bare).unwrap();
    let stdout = f.run("bare.mp4", "small.jpg", "out2.mp4").unwrap();
    assert!(stdout.contains("created udta/meta/ilst"), "{stdout}");
    f.check("bare.mp4", "small.jpg", "out2.mp4", "mjpeg,64,64", "moov is last box");
}

#[test]
fn moov_last_replace_cover() {
    let Some(f) = Fixtures::new("moovlast-replace") else { return };
    f.with_cover("base.mp4", "big.jpg", false, "in.mp4");
    let stdout = f.run("in.mp4", "small.png", "out.mp4").unwrap();
    assert!(stdout.contains("✓ Embedded artwork replaced"), "{stdout}");
    f.check("in.mp4", "small.png", "out.mp4", "png,64,64", "moov is last box");
}

#[test]
fn faststart_grow_shifts_mdat() {
    let Some(f) = Fixtures::new("faststart-grow") else { return };
    f.with_cover("base.mp4", "small.jpg", true, "in.mp4");
    f.check("in.mp4", "big.jpg", "out.mp4", "mjpeg,640,480", "mdat shifted");
    // mdat moved by exactly the moov growth.
    let a = top_level(&f.path("in.mp4"));
    let b = top_level(&f.path("out.mp4"));
    let mdat = |v: &[([u8; 4], usize, usize)]| v.iter().find(|x| &x.0 == b"mdat").unwrap().1;
    let moov = |v: &[([u8; 4], usize, usize)]| v.iter().find(|x| &x.0 == b"moov").unwrap().2;
    assert_eq!(mdat(&b) - mdat(&a), moov(&b) - moov(&a));
}

#[test]
fn faststart_shrink_resizes_free() {
    let Some(f) = Fixtures::new("faststart-shrink") else { return };
    f.with_cover("base.mp4", "big.jpg", true, "in.mp4");
    f.check("in.mp4", "small.png", "out.mp4", "png,64,64", "free box resized");
    // mdat did not move.
    let a = top_level(&f.path("in.mp4"));
    let b = top_level(&f.path("out.mp4"));
    let mdat = |v: &[([u8; 4], usize, usize)]| v.iter().find(|x| &x.0 == b"mdat").unwrap().1;
    assert_eq!(mdat(&a), mdat(&b));
}

#[test]
fn no_free_shrink_inserts_free() {
    let Some(f) = Fixtures::new("nofree") else { return };
    f.with_cover("base.mp4", "big.jpg", true, "fast.mp4");
    // Strip the `free` box between moov and mdat, fixing chunk offsets.
    let src = fs::read(f.path("fast.mp4")).unwrap();
    let top = top_level(&f.path("fast.mp4"));
    let kinds: Vec<_> = top.iter().map(|x| x.0).collect();
    assert_eq!(kinds, [*b"ftyp", *b"moov", *b"free", *b"mdat"], "unexpected faststart layout");
    let (_, free_off, free_len) = top[2];
    let mut out = src[..free_off].to_vec();
    out.extend_from_slice(&src[free_off + free_len..]);
    patch_offsets(&mut out, top[1].1, top[1].2, free_off + free_len, -(free_len as i64));
    fs::write(f.path("in.mp4"), out).unwrap();
    assert!(f.decodes_cleanly("in.mp4"), "hand-built fixture broken");

    f.check("in.mp4", "small.png", "out.mp4", "png,64,64", "free box inserted");
    let b = top_level(&f.path("out.mp4"));
    assert_eq!(b.iter().map(|x| x.0).collect::<Vec<_>>(), [*b"ftyp", *b"moov", *b"free", *b"mdat"]);
}

#[test]
fn refuses_fragmented() {
    let Some(f) = Fixtures::new("frag") else { return };
    f.ffmpeg(&["-i", "base.mp4", "-c", "copy", "-movflags", "frag_keyframe+empty_moov", "frag.mp4"]);
    let err = f.run("frag.mp4", "small.jpg", "out.mp4").unwrap_err();
    assert!(err.contains("fragmented"), "{err}");
    assert!(!f.path("out.mp4").exists());
    assert!(!f.path("out.mp4.part").exists());
}

/// Add `delta` to every stco/co64 entry >= `threshold` inside the moov at
/// `moov_off`. Test-only helper, deliberately independent of the crate.
fn patch_offsets(buf: &mut [u8], moov_off: usize, moov_len: usize, threshold: usize, delta: i64) {
    fn walk(buf: &mut [u8], start: usize, end: usize, threshold: usize, delta: i64) {
        let mut p = start;
        while p + 8 <= end {
            let s = u32::from_be_bytes(buf[p..p + 4].try_into().unwrap()) as usize;
            let t: [u8; 4] = buf[p + 4..p + 8].try_into().unwrap();
            match &t {
                b"trak" | b"mdia" | b"minf" | b"stbl" => walk(buf, p + 8, p + s, threshold, delta),
                b"stco" | b"co64" => {
                    let n = u32::from_be_bytes(buf[p + 12..p + 16].try_into().unwrap()) as usize;
                    let w = if &t == b"stco" { 4 } else { 8 };
                    for i in 0..n {
                        let q = p + 16 + i * w;
                        let v = if w == 4 {
                            u32::from_be_bytes(buf[q..q + 4].try_into().unwrap()) as u64
                        } else {
                            u64::from_be_bytes(buf[q..q + 8].try_into().unwrap())
                        };
                        if v as usize >= threshold {
                            let nv = (v as i64 + delta) as u64;
                            if w == 4 {
                                buf[q..q + 4].copy_from_slice(&(nv as u32).to_be_bytes());
                            } else {
                                buf[q..q + 8].copy_from_slice(&nv.to_be_bytes());
                            }
                        }
                    }
                }
                _ => {}
            }
            p += s;
        }
    }
    walk(buf, moov_off + 8, moov_off + moov_len, threshold, delta);
}
