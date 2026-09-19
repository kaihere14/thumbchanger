import { ProcessError } from '../processor/types'

/** A top-level ISO-BMFF box header. Mirrors `mp4::box_header::BoxHeader`. */
export interface BoxHeader {
  kind: string
  /** Absolute offset of the first byte of the size field. */
  offset: number
  /** Total box size including header. Resolved for the `size == 0` case. */
  size: number
  /** 8, or 16 when `largesize` is used. */
  headerLen: number
}

/** Scan the top-level boxes of a Blob, reading only headers. */
export async function scanTopLevel(blob: Blob): Promise<BoxHeader[]> {
  const limit = blob.size
  const boxes: BoxHeader[] = []
  let pos = 0
  while (pos < limit) {
    const head = new DataView(await blob.slice(pos, Math.min(pos + 16, limit)).arrayBuffer())
    if (head.byteLength < 8) break // trailing garbage shorter than a header: same as EOF
    const size32 = head.getUint32(0)
    const kind = fourcc(head, 4)
    let size: number
    let headerLen = 8
    if (size32 === 0) {
      size = limit - pos
    } else if (size32 === 1) {
      if (head.byteLength < 16) {
        throw new ProcessError('unsupported_container', 'The MP4 structure is invalid.', `truncated largesize header at offset ${pos}`)
      }
      size = Number(head.getBigUint64(8))
      headerLen = 16
    } else {
      size = size32
    }
    if (size < headerLen) {
      throw new ProcessError('unsupported_container', 'The MP4 structure is invalid.', `box '${kind}' at offset ${pos} has invalid size ${size}`)
    }
    if (pos + size > limit) {
      throw new ProcessError('unsupported_container', 'The MP4 structure is invalid.', `box '${kind}' at offset ${pos} (size ${size}) extends past end of file (${limit})`)
    }
    boxes.push({ kind, offset: pos, size, headerLen })
    pos += size
  }
  return boxes
}

function fourcc(view: DataView, at: number): string {
  let s = ''
  for (let i = 0; i < 4; i++) s += String.fromCharCode(view.getUint8(at + i))
  return s
}

/** Encode headers as the 21-byte records `tc_plan` expects. */
export function encodeTopLevel(boxes: BoxHeader[]): Uint8Array {
  const out = new Uint8Array(boxes.length * 21)
  const view = new DataView(out.buffer)
  boxes.forEach((b, i) => {
    const p = i * 21
    for (let k = 0; k < 4; k++) out[p + k] = b.kind.charCodeAt(k) & 0xff
    view.setBigUint64(p + 4, BigInt(b.offset), true)
    view.setBigUint64(p + 12, BigInt(b.size), true)
    out[p + 20] = b.headerLen
  })
  return out
}
