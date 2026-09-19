import { useState } from 'react'
import type { AppError } from '../types'
import { Button } from './Button'
import { SectionLabel } from './SectionLabel'

interface Props {
  error: AppError
  onRetry: () => void
  onChooseFile: () => void
}

export function ErrorPanel({ error, onRetry, onChooseFile }: Props) {
  const [showDetails, setShowDetails] = useState(false)

  return (
    <section className="error" role="alert">
      <SectionLabel tone="error">{error.title}</SectionLabel>
      <p className="error__message">{error.message}</p>
      {error.notes?.map((note) => (
        <p key={note} className="error__note">
          {note}
        </p>
      ))}

      {error.details && (
        <div className="error__details">
          <button
            type="button"
            className="link"
            aria-expanded={showDetails}
            onClick={() => setShowDetails((v) => !v)}
          >
            {showDetails ? '− details' : '+ details'}
          </button>
          {showDetails && <pre className="error__pre">{error.details}</pre>}
        </div>
      )}

      <div className="actions">
        {error.recovery === 'retry' ? (
          <>
            <Button onClick={onRetry}>try again</Button>
            <Button variant="ghost" onClick={onChooseFile}>
              choose another file
            </Button>
          </>
        ) : (
          <Button onClick={onChooseFile}>choose another file</Button>
        )}
      </div>
    </section>
  )
}
