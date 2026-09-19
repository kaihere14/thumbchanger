//! WebAssembly exports for the browser build. Plain C ABI, no bindgen: the
//! JS side (`web/src/lib/processor/wasm.ts`) copies buffers into linear
//! memory and reads results back through [`tc_result_ptr`] / [`tc_result_len`].
//!
//! The browser owns file IO (top-level scan, slicing, hashing, assembling
//! the output). This module only runs the IO-free core: planning the rewrite
//! of an in-memory `moov` and verifying the rewritten `moov`.
//!
//! All integers are little-endian.
//!
//! ## `tc_plan` input: top-level box list
//! Records of 21 bytes: `kind[4] offset:u64 size:u64 header_len:u8`.
//!
//! ## `tc_plan` result (status 0)
//! ```text
//! u8  strategy         0 SameSize, 1 MoovLast, 2 ResizeFree, 3 InsertFree, 4 ShiftAndPatch
//! i64 delta
//! u32 patched_offsets
//! u64 output_len
//! u32 covr_len
//! u32 removed_covr
//! u64 old_moov_offset
//! u64 old_moov_end
//! u16 created_len, created (utf-8, e.g. "udta/meta/ilst")
//! u32 segment_count
//!     u8 tag (0 copy: offset,len from input | 1 bytes: offset,len into blob)
//!     u64 a
//!     u64 b
//! u32 blob_len, blob
//! ```
//!
//! On a non-zero status the result is a utf-8 error message.

use std::cell::RefCell;

use crate::mp4::artwork::{self, ImageKind};
use crate::mp4::box_header::{BoxHeader, FourCc};
use crate::mp4::planner::{self, Segment, Strategy};
use crate::mp4::verify::{self, MoovExpectation};

pub const STATUS_OK: i32 = 0;
pub const STATUS_ERROR: i32 = 1;
pub const STATUS_FRAGMENTED: i32 = 2;
pub const STATUS_UNSUPPORTED_LAYOUT: i32 = 3;
pub const STATUS_BAD_IMAGE: i32 = 4;

thread_local! {
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn set_result(bytes: Vec<u8>) {
    RESULT.with(|r| *r.borrow_mut() = bytes);
}

fn fail(status: i32, err: anyhow::Error) -> i32 {
    set_result(format!("{err:#}").into_bytes());
    status
}

/// # Safety
/// Caller must free with [`tc_free`] using the same `len`.
#[unsafe(no_mangle)]
pub extern "C" fn tc_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr`/`len` must come from [`tc_alloc`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tc_free(ptr: *mut u8, len: usize) {
    drop(unsafe { Vec::from_raw_parts(ptr, 0, len.max(1)) });
}

#[unsafe(no_mangle)]
pub extern "C" fn tc_result_ptr() -> *const u8 {
    RESULT.with(|r| r.borrow().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn tc_result_len() -> usize {
    RESULT.with(|r| r.borrow().len())
}

/// # Safety
/// All pointer/length pairs must describe readable memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tc_plan(
    top_ptr: *const u8,
    top_len: usize,
    moov_ptr: *const u8,
    moov_len: usize,
    image_ptr: *const u8,
    image_len: usize,
) -> i32 {
    let top_bytes = unsafe { std::slice::from_raw_parts(top_ptr, top_len) };
    let moov = unsafe { std::slice::from_raw_parts(moov_ptr, moov_len) }.to_vec();
    let image = unsafe { std::slice::from_raw_parts(image_ptr, image_len) };

    let top = match decode_top_level(top_bytes) {
        Ok(t) => t,
        Err(e) => return fail(STATUS_ERROR, e),
    };
    let kind = match ImageKind::sniff(image) {
        Ok(k) => k,
        Err(e) => return fail(STATUS_BAD_IMAGE, e),
    };
    let covr = artwork::covr_box(kind, image);

    match planner::plan_moov(&top, moov, &covr) {
        Ok(plan) => {
            set_result(encode_plan(plan));
            STATUS_OK
        }
        Err(e) => {
            let msg = format!("{e:#}");
            let status = if msg.contains("fragmented") {
                STATUS_FRAGMENTED
            } else if msg.contains("moov") || msg.contains("mdat") {
                STATUS_UNSUPPORTED_LAYOUT
            } else {
                STATUS_ERROR
            };
            fail(status, e)
        }
    }
}

/// Compare the input and output `moov` boxes. Result on success: `u32`
/// number of chunk offsets checked.
///
/// # Safety
/// All pointer/length pairs must describe readable memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tc_verify_moov(
    in_ptr: *const u8,
    in_len: usize,
    out_ptr: *const u8,
    out_len: usize,
    covr_len: u32,
    shift: i64,
    old_moov_end: u64,
) -> i32 {
    let in_moov = unsafe { std::slice::from_raw_parts(in_ptr, in_len) };
    let out_moov = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
    let want = MoovExpectation { covr_len: covr_len as usize, shift, old_moov_end };
    match verify::check_moov_pair(in_moov, out_moov, &want) {
        Ok(n) => {
            set_result((n as u32).to_le_bytes().to_vec());
            STATUS_OK
        }
        Err(e) => fail(STATUS_ERROR, e),
    }
}

/// Fold a chunk into a running FNV-1a 64 hash (seed with [`tc_fnv1a_seed`]).
///
/// # Safety
/// `ptr`/`len` must describe readable memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tc_fnv1a(ptr: *const u8, len: usize, state: u64) -> u64 {
    verify::fnv1a_update(state, unsafe { std::slice::from_raw_parts(ptr, len) })
}

#[unsafe(no_mangle)]
pub extern "C" fn tc_fnv1a_seed() -> u64 {
    verify::FNV1A_SEED
}

fn decode_top_level(bytes: &[u8]) -> anyhow::Result<Vec<BoxHeader>> {
    const REC: usize = 21;
    if !bytes.len().is_multiple_of(REC) {
        anyhow::bail!("top-level list is {} bytes, not a multiple of {REC}", bytes.len());
    }
    Ok(bytes
        .chunks_exact(REC)
        .map(|r| BoxHeader {
            kind: FourCc([r[0], r[1], r[2], r[3]]),
            offset: u64::from_le_bytes(r[4..12].try_into().unwrap()),
            size: u64::from_le_bytes(r[12..20].try_into().unwrap()),
            header_len: r[20],
        })
        .collect())
}

fn encode_plan(plan: planner::Plan) -> Vec<u8> {
    let mut out = Vec::new();
    let strategy: u8 = match plan.strategy {
        Strategy::SameSize => 0,
        Strategy::MoovLast => 1,
        Strategy::ResizeFree => 2,
        Strategy::InsertFree => 3,
        Strategy::ShiftAndPatch => 4,
    };
    out.push(strategy);
    out.extend(plan.delta.to_le_bytes());
    out.extend((plan.patched_offsets as u32).to_le_bytes());
    out.extend(plan.output_len.to_le_bytes());
    out.extend((plan.covr_len as u32).to_le_bytes());
    out.extend((plan.rebuild.removed_covr as u32).to_le_bytes());
    out.extend(plan.old_moov.offset.to_le_bytes());
    out.extend(plan.old_moov.end().to_le_bytes());
    let created = plan.rebuild.created.join("/");
    out.extend((created.len() as u16).to_le_bytes());
    out.extend(created.as_bytes());

    let mut blob = Vec::new();
    out.extend((plan.segments.len() as u32).to_le_bytes());
    for seg in plan.segments {
        match seg {
            Segment::Copy { offset, len } => {
                out.push(0);
                out.extend(offset.to_le_bytes());
                out.extend(len.to_le_bytes());
            }
            Segment::Bytes(b) => {
                out.push(1);
                out.extend((blob.len() as u64).to_le_bytes());
                out.extend((b.len() as u64).to_le_bytes());
                blob.extend(b);
            }
        }
    }
    out.extend((blob.len() as u32).to_le_bytes());
    out.extend(blob);
    out
}
