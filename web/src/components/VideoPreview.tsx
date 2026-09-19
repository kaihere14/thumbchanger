import { formatBytes, formatDimensions, formatDuration, joinMeta } from '../lib/format'
import type { VideoAsset } from '../types'
import { Button } from './Button'
import { FileMeta } from './FileMeta'
import { SectionLabel } from './SectionLabel'

interface Props {
  video: VideoAsset
  onChange?: () => void
  disabled?: boolean
}

export function VideoPreview({ video, onChange, disabled }: Props) {
  const meta = joinMeta([
    formatBytes(video.file.size),
    video.width && video.height ? formatDimensions(video.width, video.height) : undefined,
    video.duration !== undefined ? formatDuration(video.duration) : undefined,
  ])

  return (
    <section className="pane">
      <div className="pane__head">
        <SectionLabel>video</SectionLabel>
        {onChange && (
          <Button variant="ghost" onClick={onChange} disabled={disabled}>
            change
          </Button>
        )}
      </div>
      <div className="preview">
        <video className="preview__media" src={video.url} controls muted playsInline preload="metadata" />
      </div>
      <FileMeta name={video.file.name} meta={meta} />
    </section>
  )
}
