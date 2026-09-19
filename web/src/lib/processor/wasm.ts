import wasmUrl from '../../wasm/thumbchanger.wasm?url'
import { ProcessError, type ProcessErrorCode } from './types'

/** Raw exports of `src/wasm.rs`. See that file for the ABI. */
interface Exports {
  memory: WebAssembly.Memory
  tc_alloc(len: number): number
  tc_free(ptr: number, len: number): void
  tc_result_ptr(): number
  tc_result_len(): number
  tc_plan(topPtr: number, topLen: number, moovPtr: number, moovLen: number, imgPtr: number, imgLen: number): number
  tc_verify_moov(inPtr: number, inLen: number, outPtr: number, outLen: number, covrLen: number, shift: bigint, oldMoovEnd: bigint): number
  tc_fnv1a(ptr: number, len: number, state: bigint): bigint
  tc_fnv1a_seed(): bigint
}

export type Strategy = 'SameSize' | 'MoovLast' | 'ResizeFree' | 'InsertFree' | 'ShiftAndPatch'
const STRATEGIES: Strategy[] = ['SameSize', 'MoovLast', 'ResizeFree', 'InsertFree', 'ShiftAndPatch']

export type Segment =
  | { kind: 'copy'; offset: number; len: number }
  | { kind: 'bytes'; bytes: Uint8Array }

/** Decoded `tc_plan` result. Mirrors `mp4::planner::Plan`. */
export interface Plan {
  strategy: Strategy
  delta: number
  patchedOffsets: number
  outputLen: number
  covrLen: number
  removedCovr: number
  oldMoovOffset: number
  oldMoovEnd: number
  /** Boxes created because they were missing, e.g. "udta/meta/ilst". */
  created: string
  segments: Segment[]
}

const STATUS_CODES: Record<number, ProcessErrorCode> = {
  1: 'unknown',
  2: 'fragmented',
  3: 'unsupported_container',
  4: 'unsupported_image',
}

const STATUS_MESSAGES: Record<number, string> = {
  1: 'The MP4 could not be processed.',
  2: 'Fragmented MP4 files are not supported yet.',
  3: 'This MP4 layout is not supported.',
  4: 'The thumbnail must be a JPEG or PNG image.',
}

/** Thin wrapper over the WASM core with explicit buffer management. */
export class Core {
  private readonly x: Exports

  private constructor(exports: Exports) {
    this.x = exports
  }

  static async load(): Promise<Core> {
    const response = await fetch(wasmUrl)
    if (!response.ok) throw new Error(`failed to load core: HTTP ${response.status}`)
    // Streaming compile needs `application/wasm`; fall back for hosts that
    // serve it as octet-stream.
    let instance: WebAssembly.Instance
    if (response.headers.get('content-type')?.includes('application/wasm')) {
      instance = (await WebAssembly.instantiateStreaming(response, {})).instance
    } else {
      instance = (await WebAssembly.instantiate(await response.arrayBuffer(), {})).instance
    }
    return new Core(instance.exports as unknown as Exports)
  }

  /** Memory view. Re-read after every allocation: growth detaches the buffer. */
  private mem(): Uint8Array {
    return new Uint8Array(this.x.memory.buffer)
  }

  private result(): Uint8Array {
    return this.mem().slice(this.x.tc_result_ptr(), this.x.tc_result_ptr() + this.x.tc_result_len())
  }

  private errorFromStatus(status: number): ProcessError {
    const details = new TextDecoder().decode(this.result())
    return new ProcessError(STATUS_CODES[status] ?? 'unknown', STATUS_MESSAGES[status] ?? STATUS_MESSAGES[1], details)
  }

  /** Copy `bytes` into linear memory; caller releases via the returned handle. */
  private alloc(bytes: Uint8Array): { ptr: number; len: number; free: () => void } {
    const len = bytes.byteLength
    const ptr = this.x.tc_alloc(len)
    this.mem().set(bytes, ptr)
    return { ptr, len, free: () => this.x.tc_free(ptr, len) }
  }

  plan(top: Uint8Array, moov: Uint8Array, image: Uint8Array): Plan {
    const t = this.alloc(top)
    const m = this.alloc(moov)
    const i = this.alloc(image)
    try {
      const status = this.x.tc_plan(t.ptr, t.len, m.ptr, m.len, i.ptr, i.len)
      if (status !== 0) throw this.errorFromStatus(status)
      return decodePlan(this.result())
    } finally {
      i.free()
      m.free()
      t.free()
    }
  }

  /** Returns the number of chunk offsets checked. Throws on mismatch. */
  verifyMoov(inMoov: Uint8Array, outMoov: Uint8Array, covrLen: number, shift: number, oldMoovEnd: number): number {
    const a = this.alloc(inMoov)
    const b = this.alloc(outMoov)
    try {
      const status = this.x.tc_verify_moov(a.ptr, a.len, b.ptr, b.len, covrLen, BigInt(shift), BigInt(oldMoovEnd))
      if (status !== 0) {
        const details = new TextDecoder().decode(this.result())
        throw new ProcessError('verify_failed', 'The output could not be verified.', details)
      }
      return new DataView(this.result().buffer).getUint32(0, true)
    } finally {
      b.free()
      a.free()
    }
  }

  /** Streaming FNV-1a 64 over a byte range of a Blob. */
  async hashRange(blob: Blob, offset: number, len: number, onChunk?: (done: number) => void): Promise<bigint> {
    const CHUNK = 8 << 20
    const ptr = this.x.tc_alloc(CHUNK)
    try {
      let state = this.x.tc_fnv1a_seed()
      let done = 0
      while (done < len) {
        const n = Math.min(CHUNK, len - done)
        const chunk = new Uint8Array(await blob.slice(offset + done, offset + done + n).arrayBuffer())
        if (chunk.byteLength !== n) {
          throw new ProcessError('verify_failed', 'The output could not be verified.', `short read at offset ${offset + done}`)
        }
        this.mem().set(chunk, ptr)
        state = this.x.tc_fnv1a(ptr, n, state)
        done += n
        onChunk?.(done)
      }
      return state
    } finally {
      this.x.tc_free(ptr, CHUNK)
    }
  }
}

function decodePlan(buf: Uint8Array): Plan {
  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength)
  let p = 0
  const u8 = () => buf[p++]
  const u16 = () => { const v = view.getUint16(p, true); p += 2; return v }
  const u32 = () => { const v = view.getUint32(p, true); p += 4; return v }
  const u64 = () => { const v = Number(view.getBigUint64(p, true)); p += 8; return v }
  const i64 = () => { const v = Number(view.getBigInt64(p, true)); p += 8; return v }

  const strategy = STRATEGIES[u8()]
  const delta = i64()
  const patchedOffsets = u32()
  const outputLen = u64()
  const covrLen = u32()
  const removedCovr = u32()
  const oldMoovOffset = u64()
  const oldMoovEnd = u64()
  const createdLen = u16()
  const created = new TextDecoder().decode(buf.subarray(p, p + createdLen))
  p += createdLen

  const count = u32()
  const raw: Array<{ tag: number; a: number; b: number }> = []
  for (let i = 0; i < count; i++) raw.push({ tag: u8(), a: u64(), b: u64() })
  const blobLen = u32()
  const blob = buf.slice(p, p + blobLen)

  const segments: Segment[] = raw.map((s) =>
    s.tag === 0
      ? { kind: 'copy', offset: s.a, len: s.b }
      : { kind: 'bytes', bytes: blob.subarray(s.a, s.a + s.b) },
  )

  return { strategy, delta, patchedOffsets, outputLen, covrLen, removedCovr, oldMoovOffset, oldMoovEnd, created, segments }
}
