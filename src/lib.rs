//! ThumbChanger core: replace the embedded cover artwork of an MP4 without
//! touching the media streams. The CLI (`main.rs`) and the WebAssembly
//! bindings (`wasm.rs`) are thin shells over [`mp4`].

pub mod mp4;

#[cfg(target_arch = "wasm32")]
pub mod wasm;
