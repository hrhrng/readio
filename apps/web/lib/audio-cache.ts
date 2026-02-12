interface CacheEntry {
  blob: Blob;
  duration: number;
}

export class AudioCache {
  private cache = new Map<number, CacheEntry>();
  private maxSize: number;

  constructor(maxSize: number = 50) {
    this.maxSize = maxSize;
  }

  get(key: number): CacheEntry | undefined {
    const entry = this.cache.get(key);
    if (entry) {
      // Move to end (most recently used)
      this.cache.delete(key);
      this.cache.set(key, entry);
    }
    return entry;
  }

  set(key: number, entry: CacheEntry): void {
    if (this.cache.has(key)) {
      this.cache.delete(key);
    } else if (this.cache.size >= this.maxSize) {
      // Evict oldest (first key)
      const firstKey = this.cache.keys().next().value;
      if (firstKey !== undefined) {
        const evicted = this.cache.get(firstKey);
        if (evicted) {
          URL.revokeObjectURL(URL.createObjectURL(evicted.blob));
        }
        this.cache.delete(firstKey);
      }
    }
    this.cache.set(key, entry);
  }

  has(key: number): boolean {
    return this.cache.has(key);
  }

  clear(): void {
    this.cache.clear();
  }
}
