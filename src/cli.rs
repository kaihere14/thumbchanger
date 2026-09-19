//! Command-line interface definition and argument validation.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Parser;

use thumbchanger::mp4::artwork::ImageKind;

/// Replace the embedded cover artwork of an MP4 file without re-encoding.
#[derive(Parser, Debug)]
#[command(name = "thumbchanger", version, about)]
pub struct Args {
    /// Input MP4 file.
    pub input: PathBuf,

    /// Cover image to embed (JPEG or PNG).
    #[arg(short, long, value_name = "IMAGE")]
    pub thumbnail: PathBuf,

    /// Output file. Defaults to `<input stem>-thumbchanged.mp4` next to the
    /// input. The input is never modified in place.
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,
}

/// Arguments after validation: paths exist, formats are supported,
/// output resolved.
#[derive(Debug)]
pub struct ValidatedArgs {
    pub input: PathBuf,
    pub thumbnail: PathBuf,
    pub image_kind: ImageKind,
    pub output: PathBuf,
}

const INPUT_EXTENSIONS: &[&str] = &["mp4", "m4v"];

impl Args {
    pub fn validate(self) -> Result<ValidatedArgs> {
        let input = self.input;
        if !input.is_file() {
            bail!("input file not found: {}", input.display());
        }
        let ext = input
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        match ext.as_deref() {
            Some(e) if INPUT_EXTENSIONS.contains(&e) => {}
            _ => bail!(
                "unsupported input extension (expected one of {}): {}",
                INPUT_EXTENSIONS.join(", "),
                input.display()
            ),
        }

        let thumbnail = self.thumbnail;
        if !thumbnail.is_file() {
            bail!("thumbnail file not found: {}", thumbnail.display());
        }
        let image_kind = sniff_image(&thumbnail)?;

        let output = match self.output {
            Some(p) => p,
            None => default_output(&input),
        };
        if same_file(&input, &output) {
            bail!("output must differ from input (in-place editing is not supported)");
        }

        Ok(ValidatedArgs {
            input,
            thumbnail,
            image_kind,
            output,
        })
    }
}

/// `<dir>/<stem>-thumbchanged.<ext>`
fn default_output(input: &Path) -> PathBuf {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
    let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    input.with_file_name(format!("{stem}-thumbchanged.{ext}"))
}

/// Detect image format from magic bytes, not from the extension.
fn sniff_image(path: &Path) -> Result<ImageKind> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read thumbnail: {}", path.display()))?;
    ImageKind::sniff(&bytes).with_context(|| {
        format!(
            "unsupported thumbnail format (expected JPEG or PNG): {}",
            path.display()
        )
    })
}

/// True if both paths resolve to the same existing file. If the output does
/// not exist yet, fall back to comparing canonicalized parents + file names.
fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => {
            let norm = |p: &Path| -> Option<PathBuf> {
                let parent = p.parent().filter(|d| !d.as_os_str().is_empty())?;
                let dir = fs::canonicalize(parent).ok()?;
                Some(dir.join(p.file_name()?))
            };
            match (norm(a), norm(b)) {
                (Some(x), Some(y)) => x == y,
                _ => false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_output_adds_suffix() {
        assert_eq!(
            default_output(Path::new("/x/video.mp4")),
            PathBuf::from("/x/video-thumbchanged.mp4")
        );
        assert_eq!(
            default_output(Path::new("clip.M4V")),
            PathBuf::from("clip-thumbchanged.M4V")
        );
    }
}
