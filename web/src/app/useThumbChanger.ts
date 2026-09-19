import { useCallback, useEffect, useReducer, useRef } from 'react'
import { UnsupportedFileError, loadImageAsset, loadVideoAsset } from '../lib/files'
import { ProcessError, processor } from '../lib/processor'
import type { AppError } from '../types'
import { initialState, reducer, urlsOf } from './state'

/** Own the state machine and the side effects around it (probing, processing, URL cleanup). */
export function useThumbChanger() {
  const [state, dispatch] = useReducer(reducer, initialState)
  const abortRef = useRef<AbortController | null>(null)

  // Revoke object URLs when whatever they point at leaves the state. These
  // URLs are only ever introduced by a state update, never on first mount,
  // so StrictMode's mount/unmount/mount cannot revoke a live one.
  const { video: videoUrl, thumbnail: thumbnailUrl, download: downloadUrl } = urlsOf(state)
  useEffect(() => () => { if (videoUrl) URL.revokeObjectURL(videoUrl) }, [videoUrl])
  useEffect(() => () => { if (thumbnailUrl) URL.revokeObjectURL(thumbnailUrl) }, [thumbnailUrl])
  useEffect(() => () => { if (downloadUrl) URL.revokeObjectURL(downloadUrl) }, [downloadUrl])

  // Abort an in-flight run if the component unmounts.
  useEffect(() => () => abortRef.current?.abort(), [])

  const selectVideo = useCallback(async (file: File) => {
    try {
      dispatch({ type: 'video_selected', video: await loadVideoAsset(file) })
    } catch (err) {
      const message =
        err instanceof UnsupportedFileError ? err.message : 'The file could not be read.'
      dispatch({ type: 'unsupported_video', message })
    }
  }, [])

  /** Resolves to an error string for inline display, or null on success. */
  const selectThumbnail = useCallback(async (file: File): Promise<string | null> => {
    try {
      dispatch({ type: 'thumbnail_selected', thumbnail: await loadImageAsset(file) })
      return null
    } catch (err) {
      return err instanceof UnsupportedFileError ? err.message : 'The image could not be read.'
    }
  }, [])

  const clearThumbnail = useCallback(() => dispatch({ type: 'thumbnail_cleared' }), [])

  const start = useCallback(async () => {
    if (state.status !== 'ready') return
    const controller = new AbortController()
    abortRef.current = controller
    dispatch({ type: 'process_started' })
    try {
      const result = await processor.process(state.video.file, state.thumbnail.file, {
        signal: controller.signal,
        onProgress: ({ step, progress }) => dispatch({ type: 'progress', step, progress }),
      })
      if (controller.signal.aborted) return
      dispatch({ type: 'process_succeeded', result, downloadUrl: URL.createObjectURL(result.blob) })
    } catch (err) {
      if (controller.signal.aborted) return
      dispatch({ type: 'process_failed', error: toAppError(err) })
    } finally {
      if (abortRef.current === controller) abortRef.current = null
    }
  }, [state])

  const retry = useCallback(() => dispatch({ type: 'retry' }), [])

  const reset = useCallback(() => {
    abortRef.current?.abort()
    dispatch({ type: 'reset' })
  }, [])

  return { state, selectVideo, selectThumbnail, clearThumbnail, start, retry, reset }
}

function toAppError(err: unknown): AppError {
  if (err instanceof ProcessError) {
    switch (err.code) {
      case 'verify_failed':
        return {
          title: 'error',
          message: err.message,
          notes: ['The generated file was discarded.', 'Your original file is unchanged.'],
          details: err.details,
          recovery: 'retry',
        }
      case 'fragmented':
      case 'unsupported_container':
      case 'unsupported_image':
      case 'no_moov':
        return {
          title: 'error',
          message: err.message,
          notes: ['Your original file has not been modified.'],
          details: err.details,
          recovery: 'choose_file',
        }
      default:
        return {
          title: 'error',
          message: err.message,
          notes: ['Your original file has not been modified.'],
          details: err.details,
          recovery: 'retry',
        }
    }
  }
  return {
    title: 'error',
    message: 'Something went wrong while processing the file.',
    notes: ['Your original file has not been modified.'],
    details: err instanceof Error ? `${err.name}: ${err.message}` : String(err),
    recovery: 'retry',
  }
}
