mod cli;

use std::fs;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;

use thumbchanger::mp4;
use thumbchanger::mp4::writer::WriteError;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("✗ {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args = cli::Args::parse().validate()?;

    println!("Reading input...");
    let image = fs::read(&args.thumbnail)
        .with_context(|| format!("failed to read {}", args.thumbnail.display()))?;
    let covr = mp4::artwork::covr_box(args.image_kind, &image);
    let top = mp4::parser::scan_top_level(&args.input)?;

    println!("Replacing embedded artwork...");
    let plan = mp4::planner::plan(&args.input, &top, &covr)?;

    println!("Writing output...");
    println!("Verifying...");
    let report = match mp4::writer::write(&args.input, &args.output, &plan) {
        Ok(r) => r,
        Err(WriteError::Verify(e)) => {
            eprintln!();
            eprintln!("✗ Verification failed: {e:#}");
            eprintln!();
            eprintln!("Output was NOT committed.");
            eprintln!("Temporary file removed.");
            eprintln!("Original file was not modified.");
            return Err(anyhow::anyhow!("aborted"));
        }
        Err(WriteError::Write(e)) => {
            eprintln!();
            eprintln!("✗ Write failed: {e:#}");
            eprintln!();
            eprintln!("Temporary file removed.");
            eprintln!("Original file was not modified.");
            return Err(anyhow::anyhow!("aborted"));
        }
    };

    println!();
    println!(
        "✓ Media data unchanged ({} mdat box{}, {} bytes)",
        report.mdat_count,
        if report.mdat_count == 1 { "" } else { "es" },
        report.mdat_bytes
    );
    println!("✓ Chunk offsets consistent ({} entries)", report.chunk_offsets_checked);
    println!(
        "✓ Embedded artwork {} ({:?}, {} bytes{})",
        if plan.rebuild.removed_covr > 0 { "replaced" } else { "added" },
        args.image_kind,
        image.len(),
        if plan.rebuild.created.is_empty() {
            String::new()
        } else {
            format!(", created {}", plan.rebuild.created.join("/"))
        }
    );
    println!(
        "✓ MP4 structure valid (moov {:+} bytes, {})",
        plan.delta,
        layout_note(&plan)
    );
    println!("✓ Verification passed");
    println!();
    println!("Output: {}", args.output.display());
    Ok(())
}

fn layout_note(plan: &mp4::planner::Plan) -> String {
    use mp4::planner::Strategy::*;
    match plan.strategy {
        SameSize => "same size".into(),
        MoovLast => "moov is last box, mdat untouched".into(),
        ResizeFree => "free box resized, mdat untouched".into(),
        InsertFree => "free box inserted, mdat untouched".into(),
        ShiftAndPatch => format!(
            "mdat shifted, {} chunk offsets patched",
            plan.patched_offsets
        ),
    }
}
