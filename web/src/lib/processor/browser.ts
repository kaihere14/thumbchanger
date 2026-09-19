import { encodeTopLevel, scanTopLevel, type BoxHeader } from '../mp4/boxes'
import { formatBytes, outputFileName } from '../format'
import type { StepId, VerificationCheck } from '../../types'
import { ProcessError, type Processor } from './types'
import { Core, type Plan } from './wasm'

/** Refuse to hold a `moov` larger than this in memory (same guard as the CLI, scaled for a tab). */
const MAX_MOOV_BYTES = 256 << 20

/** Share of the progress bar each step gets. Verification streams the whole file, so it dominates. */
const STEP_WEIGHT: Record<StepId, number> = {
  read: 0.04,
  analyze: 0.04,
  plan: 0.04,
  replace: 0.04,
  write: 0.04,
  verify: 0.8,
}

let corePromise: Promise<Core> | undefined
function core(): Promise<Core> {
  corePromise ??= Core.load()
  return corePromise
}

/**
 * Runs the Rust core in the browser. The file is never fully loaded: only
 * `moov` and the image are copied into memory; the output is a Blob made of
 * slices of the input plus the rewritten `moov`, exactly like the CLI's
 * segment list. Verification then re-reads both sides.
 */
export const browserProcessor: Processor = {
  async process(video, thumbnail, { onProgress, signal }) {
    const wasm = await core()
    let base = 0
    const step = (id: StepId, fraction = 0) => {
      checkAborted(signal)
      onProgress({ step: id, progress: base + STEP_WEIGHT[id] * fraction })
    }
    const finish = (id: StepId) => {
      base += STEP_WEIGHT[id]
    }

    // 1. read
    step('read')
    const image = new Uint8Array(await thumbnail.arrayBuffer())
    const top = await scanTopLevel(video)
    finish('read')

    // 2. analyze
    step('analyze')
    const moovHeader = findMoov(top)
    if (moovHeader.size > MAX_MOOV_BYTES) {
      throw new ProcessError(
        'unsupported_container',
        `The MP4 metadata is too large to process in the browser (${formatBytes(moovHeader.size)}).`,
      )
    }
    const inMoov = new Uint8Array(await video.slice(moovHeader.offset, moovHeader.offset + moovHeader.size).arrayBuffer())
    finish('analyze')

    // 3. plan + 4. replace (one call into the core)
    step('plan')
    const plan = wasm.plan(encodeTopLevel(top), inMoov, image)
    finish('plan')
    step('replace')
    finish('replace')

    // 5. write: assemble output lazily from input slices and new bytes.
    step('write')
    const parts: BlobPart[] = plan.segments.map((seg) =>
      seg.kind === 'copy' ? video.slice(seg.offset, seg.offset + seg.len) : (seg.bytes as BlobPart),
    )
    const output = new Blob(parts, { type: video.type || 'video/mp4' })
    if (output.size !== plan.outputLen) {
      throw new ProcessError('write_failed', 'The output could not be written.', `assembled ${output.size} bytes, plan expected ${plan.outputLen}`)
    }
    finish('write')

    // 6. verify
    step('verify')
    const checks = await verify(wasm, video, output, top, inMoov, plan, (f) => step('verify', f), signal)
    finish('verify')

    return {
      blob: output,
      fileName: outputFileName(video.name),
      checks,
      layoutNote: layoutNote(plan),
    }
  },
}

function findMoov(top: BoxHeader[]): BoxHeader {
  const moovs = top.filter((b) => b.kind === 'moov')
  if (moovs.length !== 1) {
    throw new ProcessError(
      'unsupported_container',
      moovs.length === 0 ? 'No MP4 metadata (moov) was found in this file.' : 'This MP4 layout is not supported.',
      `expected exactly one moov box, found ${moovs.length}`,
    )
  }
  return moovs[0]
}

/** Mirrors `mp4::verify::check`: mdat payloads identical, moov consistent with the plan. */
async function verify(
  wasm: Core,
  input: Blob,
  output: Blob,
  inTop: BoxHeader[],
  inMoov: Uint8Array,
  plan: Plan,
  onFraction: (f: number) => void,
  signal?: AbortSignal,
): Promise<VerificationCheck[]> {
  const fail = (details: string) => new ProcessError('verify_failed', 'The output could not be verified.', details)

  const outTop = await scanTopLevel(output)
  const inMdat = inTop.filter((b) => b.kind === 'mdat')
  const outMdat = outTop.filter((b) => b.kind === 'mdat')
  if (inMdat.length !== outMdat.length) throw fail(`mdat count changed: ${inMdat.length} -> ${outMdat.length}`)

  const totalBytes = inMdat.reduce((n, b) => n + (b.size - b.headerLen), 0) * 2
  let doneBytes = 0
  let mdatBytes = 0
  for (let i = 0; i < inMdat.length; i++) {
    const a = inMdat[i]
    const b = outMdat[i]
    if (a.size !== b.size || a.headerLen !== b.headerLen) throw fail(`mdat size changed: ${a.size} -> ${b.size}`)
    const len = a.size - a.headerLen
    const report = (base: number) => (n: number) => {
      checkAborted(signal)
      onFraction(totalBytes === 0 ? 1 : (base + n) / totalBytes)
    }
    const ha = await wasm.hashRange(input, a.offset + a.headerLen, len, report(doneBytes))
    doneBytes += len
    const hb = await wasm.hashRange(output, b.offset + b.headerLen, len, report(doneBytes))
    doneBytes += len
    if (ha !== hb) throw fail(`mdat payload at input offset ${a.offset} differs from output`)
    mdatBytes += len
  }

  const outMoovHeader = outTop.find((b) => b.kind === 'moov')
  if (!outMoovHeader) throw fail('output has no moov')
  const outMoov = new Uint8Array(await output.slice(outMoovHeader.offset, outMoovHeader.offset + outMoovHeader.size).arrayBuffer())
  const shift = plan.strategy === 'ShiftAndPatch' ? plan.delta : 0
  const checked = wasm.verifyMoov(inMoov, outMoov, plan.covrLen, shift, plan.oldMoovEnd)

  return [
    {
      label: 'video stream unchanged',
      detail: `${inMdat.length} mdat box${inMdat.length === 1 ? '' : 'es'}, ${formatBytes(mdatBytes)}`,
    },
    { label: 'audio stream unchanged', detail: 'same media data' },
    {
      label: `embedded artwork ${plan.removedCovr > 0 ? 'replaced' : 'added'}`,
      detail: `${formatBytes(plan.covrLen)}${plan.created ? `, created ${plan.created}` : ''}`,
    },
    {
      label: 'MP4 structure valid',
      detail: `${checked} chunk offset${checked === 1 ? '' : 's'} checked, ${layoutNote(plan)}`,
    },
  ]
}

function layoutNote(plan: Plan): string {
  switch (plan.strategy) {
    case 'SameSize':
      return 'same size'
    case 'MoovLast':
      return 'moov is last box, mdat untouched'
    case 'ResizeFree':
      return 'free box resized, mdat untouched'
    case 'InsertFree':
      return 'free box inserted, mdat untouched'
    case 'ShiftAndPatch':
      return `mdat shifted, ${plan.patchedOffsets} chunk offsets patched`
  }
}

function checkAborted(signal?: AbortSignal) {
  if (signal?.aborted) throw new ProcessError('aborted', 'Cancelled.')
}
