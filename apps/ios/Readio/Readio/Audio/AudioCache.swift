import Foundation

// MARK: - AudioCacheEntry
/// A single cached audio entry holding decoded PCM/MP3 data and its duration.
///
/// Entries are created after successfully decoding a base64 TTS response and
/// are stored in `AudioCache` for fast replay without re-synthesis.
struct AudioCacheEntry: Sendable {
    /// Raw audio data (e.g., MP3 bytes) decoded from the TTS response's `audio_base64` field.
    /// Fed directly to `AVAudioPlayer(data:)` for playback.
    let data: Data

    /// Duration of the audio clip in milliseconds, as reported by the TTS backend
    /// (from `TTSJobStatusResponse.durationMs`). Used for estimating remaining
    /// playback time before the actual `AVAudioPlayer` duration is available.
    let durationMs: Double
}

// MARK: - AudioCache
/// Thread-safe LRU (Least Recently Used) cache for decoded TTS audio data.
///
/// Port of the web app's `AudioCache` class (`lib/audio-cache.ts`), adapted for
/// iOS where we store raw `Data` instead of object URLs.
///
/// **Design decisions:**
/// - **LRU eviction:** When the cache is full, the least-recently-accessed entry is
///   evicted. This ensures frequently replayed sentences stay cached while one-off
///   prefetches are reclaimed first.
/// - **Thread safety:** All mutations are serialized on a private `NSLock`. This is
///   lightweight and avoids the overhead of a dispatch queue for simple dictionary ops.
/// - **Capacity default (50):** Matches the web app. At ~100KB per MP3 sentence this
///   means ~5MB peak memory, well within iOS limits.
/// - **Key = sentence index:** The TTS engine uses sentence indices as cache keys,
///   matching the web app's `Map<number, CacheEntry>` pattern.
///
/// **Lifecycle:**
/// - Created once per `TTSPlaybackEngine` instance.
/// - Cleared on voice or speed change (audio is speed/voice-specific).
/// - Cleared on `cleanup()` when the player is torn down.
final class AudioCache: @unchecked Sendable {

    // MARK: - Private State

    /// Ordered dictionary implemented as a plain `Dictionary` + access-order tracking.
    /// Swift's `Dictionary` doesn't preserve insertion order, so we use a separate
    /// array to track LRU ordering. The array's last element is the most-recently-used.
    private var storage: [Int: AudioCacheEntry] = [:]

    /// Access-order tracking for LRU eviction. The *first* element is the
    /// least-recently-used key; the *last* is the most-recently-used.
    private var accessOrder: [Int] = []

    /// Maximum number of entries before LRU eviction kicks in.
    private let capacity: Int

    /// Mutex for thread-safe access. `NSLock` is chosen over `os_unfair_lock`
    /// for simplicity and because cache operations are fast (no I/O).
    private let lock = NSLock()

    // MARK: - Initialization

    /// Create a new audio cache with the given capacity.
    ///
    /// - Parameter capacity: Maximum number of entries to retain. Defaults to 50,
    ///   matching the web app's `new AudioCache(50)`.
    init(capacity: Int = 50) {
        self.capacity = max(1, capacity)
    }

    // MARK: - Public API

    /// Retrieve a cached audio entry, promoting it to most-recently-used.
    ///
    /// - Parameter key: The sentence index to look up.
    /// - Returns: The cached `(data, durationMs)` tuple, or `nil` if not cached.
    func get(_ key: Int) -> (data: Data, durationMs: Double)? {
        lock.lock()
        defer { lock.unlock() }

        guard let entry = storage[key] else { return nil }

        // Promote to most-recently-used by moving to the end of accessOrder
        if let idx = accessOrder.firstIndex(of: key) {
            accessOrder.remove(at: idx)
        }
        accessOrder.append(key)

        return (entry.data, entry.durationMs)
    }

    /// Insert or replace a cached audio entry.
    ///
    /// If the cache is at capacity, the least-recently-used entry is evicted first.
    /// If an entry with the same key already exists, it is replaced in-place without
    /// changing its LRU position (it becomes most-recently-used).
    ///
    /// - Parameters:
    ///   - key: The sentence index to cache.
    ///   - data: Raw audio bytes (e.g., decoded from base64 MP3).
    ///   - durationMs: Audio duration in milliseconds.
    func set(_ key: Int, data: Data, durationMs: Double) {
        lock.lock()
        defer { lock.unlock() }

        // If key already exists, remove its old position in accessOrder
        if storage[key] != nil {
            if let idx = accessOrder.firstIndex(of: key) {
                accessOrder.remove(at: idx)
            }
        } else if storage.count >= capacity {
            // Evict the least-recently-used entry (first element in accessOrder)
            if let lruKey = accessOrder.first {
                storage.removeValue(forKey: lruKey)
                accessOrder.removeFirst()
            }
        }

        storage[key] = AudioCacheEntry(data: data, durationMs: durationMs)
        accessOrder.append(key)
    }

    /// Check whether an entry exists for the given sentence index.
    ///
    /// This does **not** promote the entry in LRU order (read-only check).
    /// Used by the prefetch logic to skip sentences that are already cached.
    ///
    /// - Parameter key: The sentence index to check.
    /// - Returns: `true` if cached, `false` otherwise.
    func has(_ key: Int) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return storage[key] != nil
    }

    /// Remove a single entry by sentence index.
    ///
    /// Used when retrying a failed sentence — the stale/corrupt entry is removed
    /// before re-fetching.
    ///
    /// - Parameter key: The sentence index to remove.
    func remove(_ key: Int) {
        lock.lock()
        defer { lock.unlock() }
        storage.removeValue(forKey: key)
        if let idx = accessOrder.firstIndex(of: key) {
            accessOrder.remove(at: idx)
        }
    }

    /// Remove all cached entries and release their memory.
    ///
    /// Called when:
    /// - Voice changes (different voice produces different audio)
    /// - Speed changes (different speed produces different audio)
    /// - The playback engine is torn down via `cleanup()`
    func clear() {
        lock.lock()
        defer { lock.unlock() }
        storage.removeAll()
        accessOrder.removeAll()
    }

    /// The current number of cached entries (for diagnostics/testing).
    var count: Int {
        lock.lock()
        defer { lock.unlock() }
        return storage.count
    }
}
