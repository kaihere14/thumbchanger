import type { VerificationCheck } from '../types'
import { SectionLabel } from './SectionLabel'

export function VerificationStatus({ checks }: { checks: VerificationCheck[] }) {
  return (
    <section className="status">
      <SectionLabel tone="success">verified</SectionLabel>
      <ul className="steps">
        {checks.map((check) => (
          <li key={check.label} className="steps__item steps__item--done">
            <span className="steps__glyph" aria-hidden="true">
              ✓
            </span>
            <span className="steps__label">
              {check.label}
              {check.detail && <span className="steps__detail"> · {check.detail}</span>}
            </span>
          </li>
        ))}
      </ul>
    </section>
  )
}
