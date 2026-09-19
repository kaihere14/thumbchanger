import { useState } from 'react'
import { IMAGE_ACCEPT } from '../lib/files'
import { formatBytes, formatDimensions, joinMeta } from '../lib/format'
import type { ImageAsset } from '../types'
import { Button } from './Button'
import { Dropzone } from './Dropzone'
import { FileMeta } from './FileMeta'
import { SectionLabel } from './SectionLabel'

interface Props {
  thumbnail?: ImageAsset
  /** Returns an inline error string, or null when the image was accepted. */
  onSelect: (file: File) => Promise<string | null>
  onClear: () => void
  disabled?: boolean
}

export function ThumbnailPicker({ thumbnail, onSelect, onClear, disabled }: Props) {
  const [error, setError] = useState<string | null>(null)

  const handleFile = async (file: File) => {
    setError(await onSelect(file))
  }

  return (
    <section className="pane">
      <div className="pane__head">
        <SectionLabel>thumbnail</SectionLabel>
        {thumbnail && !disabled && (
          <Button variant="ghost" onClick={onClear}>
            remove
          </Button>
        )}
      </div>

      <Dropzone
        accept={IMAGE_ACCEPT}
        onFile={handleFile}
        label={thumbnail ? 'drop to replace' : 'drop image here'}
        hint="or choose a .jpg / .png file"
        ariaLabel={thumbnail ? 'Replace thumbnail image' : 'Choose thumbnail image'}
        size="compact"
        disabled={disabled}
      >
        {thumbnail && (
          <div className="preview preview--image">
            <img className="preview__media" src={thumbnail.url} alt="Selected thumbnail" />
            <span className="preview__overlay">replace</span>
          </div>
        )}
      </Dropzone>

      {thumbnail ? (
        <FileMeta
          name={thumbnail.file.name}
          meta={joinMeta([
            formatDimensions(thumbnail.width, thumbnail.height),
            thumbnail.kind.toUpperCase(),
            formatBytes(thumbnail.file.size),
          ])}
        />
      ) : (
        <div className="file-meta">
          <div className="file-meta__line">JPEG or PNG · used as the embedded cover</div>
        </div>
      )}

      {error && (
        <p className="inline-error" role="alert">
          ✕ {error}
        </p>
      )}
    </section>
  )
}
