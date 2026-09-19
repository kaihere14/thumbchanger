import { useId, useRef, useState, type DragEvent, type KeyboardEvent, type ReactNode } from 'react'

interface Props {
  accept: string
  onFile: (file: File) => void
  /** Main line, e.g. "drop video here". */
  label: string
  /** Secondary line, e.g. "or choose an .mp4 file". */
  hint: string
  /** Accessible name for the control. */
  ariaLabel: string
  size?: 'large' | 'compact'
  /** Rendered instead of the label/hint, e.g. a preview that still accepts drops. */
  children?: ReactNode
  disabled?: boolean
}

/**
 * Keyboard- and drag-accessible file target. Click/Enter/Space opens the
 * picker; dragging a file over it highlights the border.
 */
export function Dropzone({
  accept,
  onFile,
  label,
  hint,
  ariaLabel,
  size = 'large',
  children,
  disabled = false,
}: Props) {
  const inputRef = useRef<HTMLInputElement>(null)
  const [dragging, setDragging] = useState(false)
  const depth = useRef(0)
  const inputId = useId()

  const open = () => {
    if (!disabled) inputRef.current?.click()
  }

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      open()
    }
  }

  const onDragEnter = (e: DragEvent) => {
    e.preventDefault()
    if (disabled) return
    depth.current++
    setDragging(true)
  }
  const onDragLeave = (e: DragEvent) => {
    e.preventDefault()
    depth.current = Math.max(0, depth.current - 1)
    if (depth.current === 0) setDragging(false)
  }
  const onDragOver = (e: DragEvent) => {
    e.preventDefault()
    if (e.dataTransfer) e.dataTransfer.dropEffect = disabled ? 'none' : 'copy'
  }
  const onDrop = (e: DragEvent) => {
    e.preventDefault()
    depth.current = 0
    setDragging(false)
    if (disabled) return
    const file = e.dataTransfer.files?.[0]
    if (file) onFile(file)
  }

  const className = [
    'dropzone',
    `dropzone--${size}`,
    dragging && 'dropzone--dragging',
    disabled && 'dropzone--disabled',
    children && 'dropzone--filled',
  ]
    .filter(Boolean)
    .join(' ')

  return (
    <div
      role="button"
      tabIndex={disabled ? -1 : 0}
      aria-label={ariaLabel}
      aria-disabled={disabled || undefined}
      className={className}
      onClick={open}
      onKeyDown={onKeyDown}
      onDragEnter={onDragEnter}
      onDragLeave={onDragLeave}
      onDragOver={onDragOver}
      onDrop={onDrop}
    >
      {children ?? (
        <div className="dropzone__text">
          <div className="dropzone__label">{label}</div>
          <div className="dropzone__hint">{hint}</div>
        </div>
      )}
      <input
        ref={inputRef}
        id={inputId}
        type="file"
        accept={accept}
        className="sr-only"
        tabIndex={-1}
        onChange={(e) => {
          const file = e.target.files?.[0]
          if (file) onFile(file)
          e.target.value = ''
        }}
      />
    </div>
  )
}
