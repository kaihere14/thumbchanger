import { formatBytes } from '../lib/format'
import type { ImageAsset, ProcessResult } from '../types'
import { Button } from './Button'
import { FileMeta } from './FileMeta'
import { SectionLabel } from './SectionLabel'
import { VerificationStatus } from './VerificationStatus'

interface Props {
  result: ProcessResult
  thumbnail: ImageAsset
  /** Object URL for `result.blob`; owned and revoked by the state hook. */
  downloadUrl: string
  onReset: () => void
}

export function Result({ result, thumbnail, downloadUrl, onReset }: Props) {

  return (
    <div className="result">
      <section className="pane">
        <SectionLabel tone="success">thumbnail changed</SectionLabel>
        <div className="preview preview--image preview--small">
          <img className="preview__media" src={thumbnail.url} alt="New embedded thumbnail" />
        </div>
        <FileMeta name={result.fileName} meta={formatBytes(result.blob.size)} />
        <p className="result__note">
          <span className="tone-success">✓</span> verified · no re-encoding
        </p>
        <div className="actions">
          <a className="btn btn--primary" href={downloadUrl} download={result.fileName}>
            download
          </a>
          <Button variant="ghost" onClick={onReset}>
            change another video
          </Button>
        </div>
      </section>

      <VerificationStatus checks={result.checks} />
    </div>
  )
}
