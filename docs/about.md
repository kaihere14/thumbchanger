# About ThumbChanger

**ThumbChanger** is a Rust-based CLI tool for changing the embedded thumbnail/cover artwork of video files without re-encoding the video or audio streams.

The initial target is **MP4**.

## Problem

Changing a video's thumbnail often leads users toward video-processing tools that re-encode the entire file.

For large videos, this creates unnecessary problems:

- Processing can take a long time.
- CPU usage is high.
- File size may change.
- Video quality can be affected if the encoding settings are not identical.
- Large files can be difficult to process efficiently.
- Many existing online tools impose restrictive file-size limits.

ThumbChanger takes a different approach: modify the **container metadata** instead of modifying the encoded media.

## Core Idea

An MP4 file can be thought of as containing several independent pieces:

```text
MP4
├── Container metadata
│   └── Cover artwork      ← modify this
│
├── Video stream           ← preserve
├── Audio stream           ← preserve
└── Media data             ← preserve
```

The goal is to replace the embedded cover artwork while leaving the encoded video and audio streams untouched.

This means:

> **No video re-encoding. No audio re-encoding. No intentional quality loss.**

However, changing metadata may still require rewriting parts of the MP4 container. "No re-encoding" does not mean "no file processing."

## Initial Scope

The first version focuses exclusively on:

- MP4 files
- JPEG/PNG thumbnails
- Embedded cover artwork
- Local CLI processing
- Large-file support
- No video/audio re-encoding

Example:

```bash
thumbchanger input.mp4 --thumbnail cover.jpg
```

Optional output path:

```bash
thumbchanger input.mp4 \
  --thumbnail cover.jpg \
  --output output.mp4
```

## Design Goals

### 1. Preserve Media Streams

The encoded video and audio streams should remain unchanged.

If the input contains:

```text
H.264 + AAC
```

the output should contain the same encoded streams.

### 2. Avoid Re-encoding

ThumbChanger should never need to decode and re-encode the video merely to change its embedded artwork.

### 3. Handle Large Files

The tool should not require loading an entire multi-gigabyte video into memory.

File-based processing and streaming should be preferred where practical.

### 4. Local Processing

The initial implementation is completely local.

The video does not need to be uploaded to a server.

This provides:

- Better privacy
- No upload bandwidth requirement
- No server storage costs
- No server-side file-size restrictions

### 5. Minimal Dependencies

Use mature libraries where they provide meaningful functionality, but avoid building an unnecessarily large dependency stack.

## Important Limitation

"Thumbnail" is not a single universal concept across video software.

Different applications may obtain a video's preview image from:

- embedded cover artwork
- the first video frame
- a keyframe
- generated/cached thumbnails
- application-specific metadata

Therefore, ThumbChanger can guarantee modification of the **embedded artwork it supports**, but cannot guarantee that every application will display that artwork as its thumbnail.

For example, a file manager and a media player may interpret the same MP4 differently.

## Architecture

The project is intended to evolve in stages:

```text
                    ThumbChanger
                         │
              ┌──────────┴──────────┐
              │                     │
             CLI              Core library
              │                     │
              └──────────┬──────────┘
                         │
                    MP4 handling
                         │
                 Container metadata
                         │
                    Cover artwork
```

The CLI is the initial interface, while the underlying functionality should eventually live in a reusable Rust library.

The same core implementation currently backs:

- CLI (`src/main.rs`)
- WebAssembly for the browser UI (`src/wasm.rs`, `web/`) — the browser owns
  file IO and hands the core only the in-memory `moov`; output is assembled
  from slices of the original file, so large files never load into memory.

It could later also back desktop or other Rust applications.

## Development Strategy

Development should proceed incrementally.

### Phase 1 — CLI

Build the Rust CLI and argument handling.

### Phase 2 — MP4 Investigation

Understand how MP4 stores embedded artwork and determine the exact boxes that need to be modified.

### Phase 3 — Proof of Concept

Use established media tooling to prove that the artwork can be changed without re-encoding the media streams.

### Phase 4 — Native Rust Implementation

Implement the minimum required MP4 container manipulation directly in Rust.

A full MP4 parser is not the goal.

### Phase 5 — Validation

Test against:

- Different H.264/H.265 files
- Different resolutions
- Existing and missing artwork
- JPEG and PNG artwork
- Large files
- Different media players and file managers

The output should also be inspected with tools such as `ffprobe` to verify that the media streams remain unchanged.

## Non-Goals

ThumbChanger is not initially intended to be:

- A video editor
- A video transcoder
- A video compressor
- A media converter
- A thumbnail generator service
- An online file-upload platform

The project has one narrow purpose:

> **Change embedded video artwork without re-encoding the media.**

## Long-Term Direction

The long-term goal is a fast, privacy-friendly tool capable of modifying video metadata efficiently, including very large files.

The ideal workflow should eventually feel as simple as:

```text
video.mp4 + thumbnail.jpg
          │
          ▼
     ThumbChanger
          │
          ▼
video-with-new-thumbnail.mp4
```

while preserving the original encoded video and audio data.