import type { Step, StepStatus } from '../types'
import { SectionLabel } from './SectionLabel'

const GLYPH: Record<StepStatus, string> = {
  done: '✓',
  active: '→',
  pending: '○',
  failed: '✕',
}

interface Props {
  steps: Step[]
  progress: number
  verifying: boolean
}

export function ProcessingStatus({ steps, progress, verifying }: Props) {
  const percent = Math.min(100, Math.round(progress * 100))
  return (
    <section className="status" aria-live="polite">
      <SectionLabel tone="accent">{verifying ? 'verifying' : 'processing'}</SectionLabel>
      <ol className="steps">
        {steps.map((step) => (
          <li key={step.id} className={`steps__item steps__item--${step.status}`}>
            <span className="steps__glyph" aria-hidden="true">
              {GLYPH[step.status]}
            </span>
            <span className="steps__label">{step.label}</span>
            <span className="sr-only">{step.status}</span>
          </li>
        ))}
      </ol>
      <div className="progress">
        <div
          className="progress__bar"
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent}
        >
          <div className="progress__fill" style={{ width: `${percent}%` }} />
        </div>
        <span className="progress__value">{percent}%</span>
      </div>
    </section>
  )
}
