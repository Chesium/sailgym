/**
 * A fixed-capacity ring buffer (task 8.4).
 *
 * The charts run for as long as the page is open, so the one property that
 * matters beyond correctness is that pushing does not allocate: the backing
 * array is built once, in the constructor, and every `push` afterwards writes
 * into a slot that already exists. `storage` exposes that array by reference
 * precisely so a test can hold onto it and prove it is never replaced.
 *
 * `toArray` does allocate — it has to, it returns a new array — which is why
 * the charts call it once per render and not once per sample.
 */
export class RingBuffer<T> {
  private readonly items: (T | undefined)[]
  /** Where the next `push` writes. */
  private cursor = 0
  private count = 0

  constructor(capacity: number) {
    if (!Number.isInteger(capacity) || capacity < 1) {
      throw new RangeError(`ring buffer capacity must be a positive integer, got ${capacity}`)
    }
    this.items = new Array<T | undefined>(capacity)
  }

  get capacity(): number {
    return this.items.length
  }

  get length(): number {
    return this.count
  }

  /**
   * The backing array, by reference.
   *
   * Exposed so `ringBuffer.test.ts` can assert that it is the same object
   * after the buffer has wrapped many times over, which is the only direct
   * evidence that `push` allocates nothing. Treat it as read-only: the slots
   * are in insertion order only until the buffer wraps.
   */
  get storage(): readonly (T | undefined)[] {
    return this.items
  }

  push(v: T): void {
    this.items[this.cursor] = v
    this.cursor = (this.cursor + 1) % this.items.length
    if (this.count < this.items.length) {
      this.count += 1
    }
  }

  /** Oldest → newest. */
  toArray(): T[] {
    const out: T[] = new Array<T>(this.count)
    const start = this.count < this.items.length ? 0 : this.cursor
    for (let i = 0; i < this.count; i += 1) {
      out[i] = this.items[(start + i) % this.items.length] as T
    }
    return out
  }

  /** The most recent value, or `undefined` while the buffer is empty. */
  get last(): T | undefined {
    if (this.count === 0) {
      return undefined
    }
    return this.items[(this.cursor - 1 + this.items.length) % this.items.length]
  }

  clear(): void {
    // The slots keep their references until they are overwritten, which costs
    // nothing and keeps `clear` allocation-free too.
    this.cursor = 0
    this.count = 0
  }
}
