//! Minimal ISO-BMFF (MP4) support: only enough to locate and replace the
//! iTunes-style cover artwork (`moov/udta/meta/ilst/covr`).
//!
//! Design contract:
//! - **Replace**: only the `covr` box.
//! - **Recalculate**: size fields of its ancestors (`ilst`, `meta`, `udta`,
//!   `moov`), plus `stco`/`co64` entries *only* if `mdat` has to move.
//! - **Preserve**: everything else byte-for-byte, including boxes we do not
//!   understand.
//!
//! Pipeline: `parser` (locate boxes) -> `planner` (decide what changes) ->
//! `writer` (apply the plan to `<output>.part`, `verify` it, rename).

pub mod artwork;
pub mod box_header;
pub mod finder;
pub mod parser;
pub mod planner;
pub mod rebuild;
pub mod verify;
pub mod writer;
