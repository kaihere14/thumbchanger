import { browserProcessor } from './browser'
import { mockProcessor } from './mock'
import type { Processor } from './types'

export * from './types'

/** The Rust core via WebAssembly. `VITE_MOCK_PROCESSOR=1` swaps in the timer-driven mock for UI work. */
export const processor: Processor = import.meta.env.VITE_MOCK_PROCESSOR ? mockProcessor : browserProcessor
