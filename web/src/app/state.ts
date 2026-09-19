import type { AppError, ImageAsset, ProcessResult, Step, StepId, VideoAsset } from '../types'
import { STEP_LABELS, STEP_ORDER } from '../lib/processor'

/**
 * One explicit state per screen. Assets are carried forward so a failed run
 * can be retried without re-selecting files.
 */
export type AppState =
  | { status: 'idle' }
  | { status: 'video_selected'; video: VideoAsset }
  | { status: 'ready'; video: VideoAsset; thumbnail: ImageAsset }
  | { status: 'processing'; video: VideoAsset; thumbnail: ImageAsset; steps: Step[]; progress: number }
  | { status: 'verifying'; video: VideoAsset; thumbnail: ImageAsset; steps: Step[]; progress: number }
  | { status: 'success'; video: VideoAsset; thumbnail: ImageAsset; result: ProcessResult; downloadUrl: string }
  | { status: 'error'; video?: VideoAsset; thumbnail?: ImageAsset; error: AppError }

export type AppAction =
  | { type: 'video_selected'; video: VideoAsset }
  | { type: 'thumbnail_selected'; thumbnail: ImageAsset }
  | { type: 'thumbnail_cleared' }
  | { type: 'process_started' }
  | { type: 'progress'; step: StepId; progress: number }
  | { type: 'process_succeeded'; result: ProcessResult; downloadUrl: string }
  | { type: 'process_failed'; error: AppError }
  | { type: 'unsupported_video'; message: string }
  | { type: 'retry' }
  | { type: 'reset' }

export const initialState: AppState = { status: 'idle' }

function freshSteps(): Step[] {
  return STEP_ORDER.map((id) => ({ id, label: STEP_LABELS[id], status: 'pending' }))
}

function advanceSteps(steps: Step[], active: StepId): Step[] {
  const activeIndex = STEP_ORDER.indexOf(active)
  return steps.map((step, i) => ({
    ...step,
    status: i < activeIndex ? 'done' : i === activeIndex ? 'active' : 'pending',
  }))
}

export function reducer(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case 'video_selected':
      return { status: 'video_selected', video: action.video }

    case 'thumbnail_selected': {
      const video = 'video' in state ? state.video : undefined
      if (!video) return state
      return { status: 'ready', video, thumbnail: action.thumbnail }
    }

    case 'thumbnail_cleared':
      if (state.status !== 'ready') return state
      return { status: 'video_selected', video: state.video }

    case 'process_started':
      if (state.status !== 'ready') return state
      return {
        status: 'processing',
        video: state.video,
        thumbnail: state.thumbnail,
        steps: advanceSteps(freshSteps(), 'read'),
        progress: 0,
      }

    case 'progress': {
      if (state.status !== 'processing' && state.status !== 'verifying') return state
      return {
        ...state,
        status: action.step === 'verify' ? 'verifying' : 'processing',
        steps: advanceSteps(state.steps, action.step),
        progress: action.progress,
      }
    }

    case 'process_succeeded':
      if (state.status !== 'processing' && state.status !== 'verifying') return state
      return {
        status: 'success',
        video: state.video,
        thumbnail: state.thumbnail,
        result: action.result,
        downloadUrl: action.downloadUrl,
      }

    case 'process_failed':
      return {
        status: 'error',
        video: 'video' in state ? state.video : undefined,
        thumbnail: 'thumbnail' in state ? state.thumbnail : undefined,
        error: action.error,
      }

    case 'unsupported_video':
      return {
        status: 'error',
        error: {
          title: 'unsupported file',
          message: action.message,
          recovery: 'choose_file',
        },
      }

    case 'retry':
      if (state.status !== 'error' || !state.video) return { status: 'idle' }
      if (state.thumbnail) return { status: 'ready', video: state.video, thumbnail: state.thumbnail }
      return { status: 'video_selected', video: state.video }

    case 'reset':
      return initialState
  }
}

/** Object URLs currently held by the state, for cleanup. */
export function urlsOf(state: AppState): { video?: string; thumbnail?: string; download?: string } {
  return {
    video: 'video' in state ? state.video?.url : undefined,
    thumbnail: 'thumbnail' in state ? state.thumbnail?.url : undefined,
    download: state.status === 'success' ? state.downloadUrl : undefined,
  }
}
