import { useEffect } from 'react'
import { Button } from '../components/Button'
import { Dropzone } from '../components/Dropzone'
import { ErrorPanel } from '../components/ErrorPanel'
import { Footer } from '../components/Footer'
import { Header } from '../components/Header'
import { ProcessingStatus } from '../components/ProcessingStatus'
import { Result } from '../components/Result'
import { SectionLabel } from '../components/SectionLabel'
import { ThumbnailPicker } from '../components/ThumbnailPicker'
import { VideoPreview } from '../components/VideoPreview'
import { VIDEO_ACCEPT } from '../lib/files'
import { useThumbChanger } from './useThumbChanger'

export function App() {
  const { state, selectVideo, selectThumbnail, clearThumbnail, start, retry, reset } =
    useThumbChanger()

  // Stop the browser from navigating to a file dropped outside a dropzone.
  useEffect(() => {
    const block = (e: globalThis.DragEvent) => e.preventDefault()
    window.addEventListener('dragover', block)
    window.addEventListener('drop', block)
    return () => {
      window.removeEventListener('dragover', block)
      window.removeEventListener('drop', block)
    }
  }, [])

  return (
    <div className="shell">
      <Header />
      <main className="main">{renderScreen()}</main>
      <Footer />
    </div>
  )

  function renderScreen() {
    switch (state.status) {
      case 'idle':
        return (
          <div className="screen screen--idle">
            <SectionLabel>change video thumbnail</SectionLabel>
            <Dropzone
              accept={VIDEO_ACCEPT}
              onFile={selectVideo}
              label="drop video here"
              hint="or choose an .mp4 file"
              ariaLabel="Choose an MP4 video"
              size="large"
            />
            <p className="hint">MP4 only · processed locally · no re-encoding</p>
          </div>
        )

      case 'video_selected':
      case 'ready': {
        const ready = state.status === 'ready'
        return (
          <div className="screen">
            <div className="workspace">
              <VideoPreview video={state.video} onChange={reset} />
              <ThumbnailPicker
                thumbnail={ready ? state.thumbnail : undefined}
                onSelect={selectThumbnail}
                onClear={clearThumbnail}
              />
            </div>
            <div className="actions actions--primary">
              <Button variant="primary" onClick={start} disabled={!ready}>
                change thumbnail
              </Button>
              <span className="actions__hint">
                {ready ? 'writes a new file · original untouched' : 'choose a thumbnail to continue'}
              </span>
            </div>
          </div>
        )
      }

      case 'processing':
      case 'verifying':
        return (
          <div className="screen">
            <div className="workspace workspace--muted" aria-hidden="true">
              <VideoPreview video={state.video} disabled />
              <ThumbnailPicker
                thumbnail={state.thumbnail}
                onSelect={selectThumbnail}
                onClear={clearThumbnail}
                disabled
              />
            </div>
            <ProcessingStatus
              steps={state.steps}
              progress={state.progress}
              verifying={state.status === 'verifying'}
            />
          </div>
        )

      case 'success':
        return (
          <div className="screen">
            <Result
              result={state.result}
              thumbnail={state.thumbnail}
              downloadUrl={state.downloadUrl}
              onReset={reset}
            />
          </div>
        )

      case 'error':
        return (
          <div className="screen">
            <ErrorPanel error={state.error} onRetry={retry} onChooseFile={reset} />
          </div>
        )
    }
  }
}
