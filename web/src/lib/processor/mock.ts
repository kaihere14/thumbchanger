import type { ProcessResult, StepId } from '../../types'
import { formatBytes, outputFileName } from '../format'
import { ProcessError, type ProcessOptions, type Processor } from './types'

/**
 * Temporary stand-in for the Rust/WASM core. Walks the real pipeline steps
 * on a timer and returns the input bytes unchanged as the "output".
 *
 * Failure paths can be exercised by file name, for demoing the UI:
 *   - video name contains `frag`  → fragmented MP4 error
 *   - image name contains `fail`  → verification failure
 */
export const mockProcessor: Processor = {
  async process(video, thumbnail, { onProgress, signal }) {
    const steps: Array<[StepId, number]> = [
      ['read', 300],
      ['analyze', 500],
      ['plan', 350],
      ['replace', 450],
      ['write', 1400],
      ['verify', 900],
    ]
    const total = steps.reduce((sum, [, ms]) => sum + ms, 0)
    let elapsed = 0

    for (const [step, ms] of steps) {
      onProgress({ step, progress: elapsed / total })
      await tick(ms, elapsed, total, onProgress, step, signal)
      elapsed += ms

      if (step === 'analyze' && /frag/i.test(video.name)) {
        throw new ProcessError(
          'fragmented',
          'Fragmented MP4 files are not supported yet.',
          'moov contains mvex; fragmented (fMP4/DASH) layouts are out of scope for v0.1',
        )
      }
      if (step === 'verify' && /fail/i.test(thumbnail.name)) {
        throw new ProcessError(
          'verify_failed',
          'The output could not be verified.',
          'mdat payload at input offset 48 differs from output',
        )
      }
    }

    const result: ProcessResult = {
      blob: video,
      fileName: outputFileName(video.name),
      checks: [
        { label: 'video stream unchanged', detail: `1 mdat box, ${formatBytes(video.size)}` },
        { label: 'audio stream unchanged' },
        {
          label: 'embedded artwork replaced',
          detail: `${thumbnail.type.replace('image/', '') || 'image'}, ${formatBytes(thumbnail.size)}`,
        },
        { label: 'MP4 structure valid', detail: 'moov is last box, mdat untouched' },
      ],
      layoutNote: 'moov is last box, mdat untouched',
    }
    return result
  },
}

/** Sleep `ms`, emitting a few intermediate progress events for the bar. */
async function tick(
  ms: number,
  elapsed: number,
  total: number,
  onProgress: ProcessOptions['onProgress'],
  step: StepId,
  signal?: AbortSignal,
) {
  const slices = 6
  for (let i = 1; i <= slices; i++) {
    await sleep(ms / slices, signal)
    onProgress({ step, progress: (elapsed + (ms * i) / slices) / total })
  }
}

function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) return reject(new ProcessError('aborted', 'Cancelled.'))
    const id = setTimeout(() => {
      signal?.removeEventListener('abort', onAbort)
      resolve()
    }, ms)
    function onAbort() {
      clearTimeout(id)
      reject(new ProcessError('aborted', 'Cancelled.'))
    }
    signal?.addEventListener('abort', onAbort, { once: true })
  })
}
