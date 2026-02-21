interface CacheEntry {
  url: string; // Object URL — created by caller via URL.createObjectURL()
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
      // Replace existing — revoke the old URL first
      const old = this.cache.get(key)!;
      URL.revokeObjectURL(old.url);
      this.cache.delete(key);
    } else if (this.cache.size >= this.maxSize) {
      // Evict oldest (first key) and revoke its URL
      const firstKey = this.cache.keys().next().value;
      if (firstKey !== undefined) {
        const evicted = this.cache.get(firstKey);
        if (evicted) {
          URL.revokeObjectURL(evicted.url);
        }
        this.cache.delete(firstKey);
      }
    }
    this.cache.set(key, entry);
  }

  has(key: number): boolean {
    return this.cache.has(key);
  }

  /** Remove a single entry by key, revoking its object URL. */
  delete(key: number): boolean {
    const entry = this.cache.get(key);
    if (entry) {
      URL.revokeObjectURL(entry.url);
      this.cache.delete(key);
      return true;
    }
    return false;
  }

  clear(): void {
    // Revoke every cached object URL before dropping the entries
    for (const entry of this.cache.values()) {
      URL.revokeObjectURL(entry.url);
    }
    this.cache.clear();
  }
}
