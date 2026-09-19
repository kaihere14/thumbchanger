const UNITS = ['B', 'KB', 'MB', 'GB', 'TB']

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  let value = bytes
  let i = 0
  while (value >= 1024 && i < UNITS.length - 1) {
    value /= 1024
    i++
  }
  const digits = value >= 100 ? 0 : 1
  return `${value.toFixed(digits)} ${UNITS[i]}`
}

export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds)) return '--:--'
  const total = Math.round(seconds)
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  const mm = h > 0 ? String(m).padStart(2, '0') : String(m)
  const ss = String(s).padStart(2, '0')
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`
}

export function formatDimensions(width: number, height: number): string {
  return `${width} × ${height}`
}

/** Mirrors the CLI default: `<stem>-thumbchanged.<ext>`. */
export function outputFileName(inputName: string): string {
  const dot = inputName.lastIndexOf('.')
  if (dot <= 0) return `${inputName}-thumbchanged.mp4`
  return `${inputName.slice(0, dot)}-thumbchanged${inputName.slice(dot)}`
}

export function joinMeta(parts: Array<string | undefined | null | false>): string {
  return parts.filter(Boolean).join(' · ')
}
