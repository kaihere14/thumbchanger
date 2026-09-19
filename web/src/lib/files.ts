import type { ImageAsset, ImageKind, VideoAsset } from '../types'

export const VIDEO_EXTENSIONS = ['mp4', 'm4v']
export const VIDEO_ACCEPT = '.mp4,.m4v,video/mp4'
export const IMAGE_ACCEPT = '.jpg,.jpeg,.png,image/jpeg,image/png'

function extensionOf(name: string): string {
  const dot = name.lastIndexOf('.')
  return dot < 0 ? '' : name.slice(dot + 1).toLowerCase()
}

/** Extension check only. The real container check happens in the processor. */
export function isSupportedVideo(file: File): boolean {
  return VIDEO_EXTENSIONS.includes(extensionOf(file.name))
}

/** Detect JPEG/PNG from magic bytes, not from the extension or MIME type. */
export async function sniffImageKind(file: File): Promise<ImageKind | null> {
  const head = new Uint8Array(await file.slice(0, 8).arrayBuffer())
  if (head.length >= 3 && head[0] === 0xff && head[1] === 0xd8 && head[2] === 0xff) return 'jpeg'
  const png = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]
  if (head.length >= 8 && png.every((b, i) => head[i] === b)) return 'png'
  return null
}

export class UnsupportedFileError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'UnsupportedFileError'
  }
}

export async function loadImageAsset(file: File): Promise<ImageAsset> {
  const kind = await sniffImageKind(file)
  if (!kind) {
    throw new UnsupportedFileError('ThumbChanger currently supports JPEG and PNG images.')
  }
  const url = URL.createObjectURL(file)
  try {
    const { width, height } = await readImageSize(url)
    return { file, url, kind, width, height }
  } catch (err) {
    URL.revokeObjectURL(url)
    throw err
  }
}

function readImageSize(url: string): Promise<{ width: number; height: number }> {
  return new Promise((resolve, reject) => {
    const img = new Image()
    img.onload = () => resolve({ width: img.naturalWidth, height: img.naturalHeight })
    img.onerror = () => reject(new UnsupportedFileError('The image could not be decoded.'))
    img.src = url
  })
}

export async function loadVideoAsset(file: File): Promise<VideoAsset> {
  if (!isSupportedVideo(file)) {
    throw new UnsupportedFileError('ThumbChanger currently supports MP4 files.')
  }
  const url = URL.createObjectURL(file)
  const meta = await readVideoMetadata(url)
  return { file, url, ...meta }
}

/**
 * Best-effort probe via a detached `<video>` element. Failure is not fatal:
 * the browser may not decode the codec even though the container is fine.
 */
function readVideoMetadata(
  url: string,
): Promise<Pick<VideoAsset, 'duration' | 'width' | 'height'>> {
  return new Promise((resolve) => {
    const video = document.createElement('video')
    video.preload = 'metadata'
    video.muted = true
    const done = (meta: Pick<VideoAsset, 'duration' | 'width' | 'height'>) => {
      video.removeAttribute('src')
      video.load()
      resolve(meta)
    }
    video.onloadedmetadata = () =>
      done({
        duration: Number.isFinite(video.duration) ? video.duration : undefined,
        width: video.videoWidth || undefined,
        height: video.videoHeight || undefined,
      })
    video.onerror = () => done({})
    video.src = url
  })
}
