# ThumbChanger — web UI

Browser frontend for ThumbChanger. React + TypeScript + Vite, no other runtime
dependencies. Processing runs the Rust core compiled to WebAssembly.

```bash
pnpm install
pnpm build:wasm   # needs `rustup target add wasm32-unknown-unknown`; refreshes src/wasm/thumbchanger.wasm
pnpm dev          # http://localhost:5173
pnpm build        # type-check + bundle into dist/
pnpm lint
```

## Layout

```text
src/
├── app/
│   ├── App.tsx              screen per state
│   ├── state.ts             reducer: idle → video_selected → ready → processing → verifying → success | error
│   └── useThumbChanger.ts   side effects: file probing, running the processor, object URL cleanup
├── components/              Header, Footer, Dropzone, VideoPreview, ThumbnailPicker,
│                            ProcessingStatus, VerificationStatus, Result, ErrorPanel, …
├── lib/
│   ├── files.ts             extension / magic-byte validation, `<video>` and `<img>` probing
│   ├── format.ts            bytes, durations, output file name
│   ├── mp4/boxes.ts         top-level box scan over a Blob (headers only)
│   └── processor/
│       ├── types.ts         `Processor` interface, `ProcessError`, step ids and labels
│       ├── browser.ts       real processor: scan → plan (WASM) → assemble Blob → verify
│       ├── wasm.ts          loader + C-ABI glue for `src/wasm.rs`
│       ├── mock.ts          timer-driven stand-in, for UI work
│       └── index.ts         exports the active processor
├── styles/global.css
├── types/
└── wasm/thumbchanger.wasm   built from ../src via `pnpm build:wasm`
```

## Processing seam

The UI only talks to `processor.process(video, thumbnail, { onProgress, signal })`
from `src/lib/processor`. It resolves with a `ProcessResult` (output blob,
file name, verification checks) or rejects with a `ProcessError` carrying a
user-facing message, a code, and optional technical details.

## How the browser path works

The video is never loaded whole. `browser.ts` reads the top-level box headers,
copies only `moov` and the image into WASM memory, and asks the core
(`planner::plan_moov`) for a plan. The output is a `Blob` assembled from
slices of the input `File` plus the rewritten `moov`, so a multi-GB file
costs no memory. Verification then hashes every `mdat` payload on both sides
(`tc_fnv1a`) and checks the output `moov` against the plan
(`verify::check_moov_pair`) — the same checks the CLI runs. The output is
byte-identical to what the CLI writes.

## Mock processor

`VITE_MOCK_PROCESSOR=1 pnpm dev` swaps in a timer-driven mock that returns
the input unchanged. Failures can be triggered by file name:

- video name containing `frag` → "Fragmented MP4 files are not supported yet."
- image name containing `fail` → "The output could not be verified."
