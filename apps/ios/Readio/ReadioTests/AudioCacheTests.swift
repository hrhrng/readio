import XCTest
@testable import Readio

// ---------------------------------------------------------------------------
// MARK: - AudioCache unit tests
// Validates LRU eviction policy, capacity enforcement, and basic operations.
// ---------------------------------------------------------------------------

final class AudioCacheTests: XCTestCase {

    /// Helper to create dummy audio data of a given byte count.
    private func dummyData(_ byte: UInt8 = 0, count: Int = 100) -> Data {
        Data(repeating: byte, count: count)
    }

    // MARK: - Basic operations

    /// Storing and retrieving an entry should return the same data.
    func testSetAndGet() {
        let cache = AudioCache(capacity: 10)
        let data = dummyData(0xAB)

        cache.set(0, data: data, durationMs: 1500.0)
        let entry = cache.get(0)

        XCTAssertNotNil(entry)
        XCTAssertEqual(entry?.data, data)
        XCTAssertEqual(entry?.durationMs, 1500.0)
    }

    /// Getting a key that was never stored should return nil.
    func testGetMissing() {
        let cache = AudioCache(capacity: 10)
        XCTAssertNil(cache.get(42))
    }

    /// `has(_:)` should reflect whether a key exists.
    func testHas() {
        let cache = AudioCache(capacity: 10)
        XCTAssertFalse(cache.has(0))

        cache.set(0, data: dummyData(), durationMs: 1000)
        XCTAssertTrue(cache.has(0))
    }

    /// `remove(_:)` should delete a specific entry.
    func testRemove() {
        let cache = AudioCache(capacity: 10)
        cache.set(0, data: dummyData(), durationMs: 1000)
        cache.set(1, data: dummyData(), durationMs: 2000)

        cache.remove(0)

        XCTAssertFalse(cache.has(0))
        XCTAssertTrue(cache.has(1))
    }

    /// `clear()` should remove all entries.
    func testClear() {
        let cache = AudioCache(capacity: 10)
        for i in 0..<5 {
            cache.set(i, data: dummyData(UInt8(i)), durationMs: Double(i * 1000))
        }

        cache.clear()

        for i in 0..<5 {
            XCTAssertFalse(cache.has(i), "Key \(i) should be cleared")
        }
    }

    // MARK: - LRU eviction

    /// When capacity is exceeded, the least-recently-used entry should be evicted.
    func testLRUEviction() {
        let cache = AudioCache(capacity: 3)

        // Fill to capacity: keys 0, 1, 2
        cache.set(0, data: dummyData(0), durationMs: 100)
        cache.set(1, data: dummyData(1), durationMs: 200)
        cache.set(2, data: dummyData(2), durationMs: 300)

        // Adding key 3 should evict key 0 (oldest/LRU)
        cache.set(3, data: dummyData(3), durationMs: 400)

        XCTAssertFalse(cache.has(0), "Key 0 should have been evicted")
        XCTAssertTrue(cache.has(1))
        XCTAssertTrue(cache.has(2))
        XCTAssertTrue(cache.has(3))
    }

    /// Accessing an entry should promote it, preventing eviction.
    func testLRUPromotion() {
        let cache = AudioCache(capacity: 3)

        cache.set(0, data: dummyData(0), durationMs: 100)
        cache.set(1, data: dummyData(1), durationMs: 200)
        cache.set(2, data: dummyData(2), durationMs: 300)

        // Access key 0 to promote it (it was the oldest)
        _ = cache.get(0)

        // Adding key 3 should now evict key 1 (the new LRU) instead of key 0
        cache.set(3, data: dummyData(3), durationMs: 400)

        XCTAssertTrue(cache.has(0), "Key 0 was accessed recently, should survive")
        XCTAssertFalse(cache.has(1), "Key 1 should have been evicted as new LRU")
        XCTAssertTrue(cache.has(2))
        XCTAssertTrue(cache.has(3))
    }

    /// Overwriting an existing key should update the value and promote it.
    func testOverwritePromotes() {
        let cache = AudioCache(capacity: 3)

        cache.set(0, data: dummyData(0), durationMs: 100)
        cache.set(1, data: dummyData(1), durationMs: 200)
        cache.set(2, data: dummyData(2), durationMs: 300)

        // Overwrite key 0 with new data — should promote to most-recently-used
        let newData = dummyData(0xFF)
        cache.set(0, data: newData, durationMs: 999)

        // Adding key 3 should evict key 1 (the LRU), not key 0
        cache.set(3, data: dummyData(3), durationMs: 400)

        XCTAssertTrue(cache.has(0))
        XCTAssertEqual(cache.get(0)?.durationMs, 999, "Duration should be updated")
        XCTAssertFalse(cache.has(1), "Key 1 should be evicted")
    }

    // MARK: - Edge cases

    /// Cache with capacity 1 should always hold only the most recent entry.
    func testCapacityOne() {
        let cache = AudioCache(capacity: 1)

        cache.set(0, data: dummyData(0), durationMs: 100)
        XCTAssertTrue(cache.has(0))

        cache.set(1, data: dummyData(1), durationMs: 200)
        XCTAssertFalse(cache.has(0))
        XCTAssertTrue(cache.has(1))
    }

    /// Removing a non-existent key should not crash.
    func testRemoveNonExistent() {
        let cache = AudioCache(capacity: 10)
        cache.remove(999) // Should not crash
        XCTAssertFalse(cache.has(999))
    }
}
