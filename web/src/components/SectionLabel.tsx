import type { ReactNode } from 'react'

type Tone = 'default' | 'accent' | 'success' | 'error'

export function SectionLabel({ children, tone = 'default' }: { children: ReactNode; tone?: Tone }) {
  return <h2 className={`section-label section-label--${tone}`}>{children}</h2>
}
