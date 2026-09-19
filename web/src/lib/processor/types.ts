import type { ProcessResult, StepId } from '../../types'

export interface ProgressEvent {
  /** Step that just became active. */
  step: StepId
  /** Overall completion, 0..1. */
  progress: number
}

export interface ProcessOptions {
  onProgress: (event: ProgressEvent) => void
  signal?: AbortSignal
}

/**
 * The single seam between the UI and the MP4 core. The mock implementation
 * lives in `mock.ts`; the Rust/WASM implementation will replace it behind
 * the same interface.
 */
export interface Processor {
  process(video: File, thumbnail: File, options: ProcessOptions): Promise<ProcessResult>
}

export type ProcessErrorCode =
  | 'unsupported_container'
  | 'unsupported_image'
  | 'fragmented'
  | 'no_moov'
  | 'write_failed'
  | 'verify_failed'
  | 'aborted'
  | 'unknown'

/** Failure surfaced to the user. `message` is plain language; `details` is technical. */
export class ProcessError extends Error {
  readonly code: ProcessErrorCode
  readonly details?: string

  constructor(code: ProcessErrorCode, message: string, details?: string) {
    super(message)
    this.name = 'ProcessError'
    this.code = code
    this.details = details
  }
}

export const STEP_ORDER: StepId[] = ['read', 'analyze', 'plan', 'replace', 'write', 'verify']

export const STEP_LABELS: Record<StepId, string> = {
  read: 'reading input',
  analyze: 'analyzing MP4',
  plan: 'planning rewrite',
  replace: 'replacing artwork',
  write: 'writing output',
  verify: 'verifying output',
}
