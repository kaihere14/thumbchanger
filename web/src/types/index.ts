/** A video file the user selected, plus what the browser could learn about it. */
export interface VideoAsset {
  file: File
  /** Object URL for `<video>` preview. Revoked when the asset is discarded. */
  url: string
  /** Seconds. Undefined if the browser could not decode the container. */
  duration?: number
  width?: number
  height?: number
}

export type ImageKind = 'jpeg' | 'png'

/** A thumbnail image the user selected. `kind` is sniffed from magic bytes. */
export interface ImageAsset {
  file: File
  url: string
  kind: ImageKind
  width: number
  height: number
}

export type StepId = 'read' | 'analyze' | 'plan' | 'replace' | 'write' | 'verify'

export type StepStatus = 'pending' | 'active' | 'done' | 'failed'

export interface Step {
  id: StepId
  label: string
  status: StepStatus
}

export interface VerificationCheck {
  label: string
  /** Optional technical detail, e.g. "2 mdat boxes, 1.8 GB". */
  detail?: string
}

export interface ProcessResult {
  blob: Blob
  fileName: string
  checks: VerificationCheck[]
  /** One-line summary of the container rewrite, e.g. "moov is last box, mdat untouched". */
  layoutNote?: string
}

export type ErrorRecovery = 'choose_file' | 'retry'

export interface AppError {
  title: string
  message: string
  /** Extra reassurance lines shown under the message. */
  notes?: string[]
  /** Technical details, behind a toggle. Never shown by default. */
  details?: string
  recovery: ErrorRecovery
}
