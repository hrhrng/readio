import Foundation

#if canImport(Readio)
@testable import Readio
#endif

#if canImport(XCTest)
import XCTest

final class ReadioSDKTests: XCTestCase {
    func testReplicaImportsAChangeProducedByAnotherAppleSDKInstance() throws {
        let bookmark = try convergedBookmark()
        XCTAssertEqual(bookmark["item_id"] as? String, "book-1")
        XCTAssertEqual(bookmark["char_offset"] as? Int, 18)
    }

    func testReplicaRestoresItsLoroSnapshotAfterAnAppRestart() throws {
        let bookmark = try restoredBookmark()
        XCTAssertEqual(bookmark["item_id"] as? String, "book-2")
        XCTAssertEqual(bookmark["char_offset"] as? Int, 41)
    }

    func testSDKUsesTheSharedRustStorageProjectionForSyncPushes() throws {
        let plan = try syncPushPlan()
        XCTAssertEqual(plan["cursor"] as? Int, 1)
        let results = try XCTUnwrap(plan["results"] as? [[String: Any]])
        XCTAssertEqual(results.first?["status"] as? String, "applied")
    }

    func testSyncStateStagesAndProjectsThroughThePrivateCABI() throws {
        var state = ReadioSDK().makeSyncState()
        try state.stageLocal(
            entitiesJSON: #"[{"entityKind":"reading_position","entityId":"book-1","payload":{"char_offset":42}}]"#
        )
        let view = try state.viewJSON()
        XCTAssertTrue(view.contains("reading_positions"))
        XCTAssertTrue(view.contains("headsBase64"))
    }
}
#else
@main
enum ReadioSDKTestRunner {
    static func main() throws {
        let bookmark = try convergedBookmark()
        guard bookmark["item_id"] as? String == "book-1" else {
            fatalError("the imported bookmark must retain its item id")
        }
        guard bookmark["char_offset"] as? Int == 18 else {
            fatalError("the imported bookmark must retain its scalar offset")
        }

        let restored = try restoredBookmark()
        guard restored["item_id"] as? String == "book-2" else {
            fatalError("the restored snapshot must retain its item id")
        }
        guard restored["char_offset"] as? Int == 41 else {
            fatalError("the restored snapshot must retain its scalar offset")
        }

        let plan = try syncPushPlan()
        guard plan["cursor"] as? Int == 1 else {
            fatalError("the shared sync projection must advance the cursor")
        }
        let results = try unwrap(plan["results"] as? [[String: Any]])
        guard results.first?["status"] as? String == "applied" else {
            fatalError("the shared sync projection must apply the mutation")
        }
    }
}

private func syncPushPlan() throws -> [String: Any] {
    let sdk = ReadioSDK()
    let planJSON = try sdk.planSyncPush(
        snapshotJSON: #"{"entities":[],"mutations":[]}"#,
        batchJSON: #"{"device_id":"apple-a","mutations":[{"mutation_id":"m-1","entity_kind":"setting","entity_id":"theme","operation":"upsert","base_version":0,"client_timestamp_unix_ms":1,"payload":{"value":"dark"}}]}"#,
        serverTimestampUnixMilliseconds: 10,
        startingCursor: 0
    )
    return try unwrap(
        JSONSerialization.jsonObject(with: Data(planJSON.utf8)) as? [String: Any]
    )
}

private func restoredBookmark() throws -> [String: Any] {
    let sdk = ReadioSDK()
    let original = sdk.makeReplica()
    _ = try original.upsert(
        kind: .bookmark,
        entityID: "bookmark-2",
        payloadJSON: #"{"item_id":"book-2","char_offset":41}"#
    )

    let snapshot = try original.snapshotBase64()
    let restored = try sdk.makeReplica(snapshotBase64: snapshot)
    guard try restored.headsBase64() == original.headsBase64() else {
        throw ReadioSDKTestError.missingValue
    }
    let state = try unwrap(
        JSONSerialization.jsonObject(with: Data(try restored.stateJSON().utf8))
            as? [String: Any]
    )
    let bookmarks = try unwrap(state["bookmarks"] as? [String: Any])
    return try unwrap(bookmarks["bookmark-2"] as? [String: Any])
}
#endif

private func convergedBookmark() throws -> [String: Any] {
    let sdk = ReadioSDK()
    let desktop = sdk.makeReplica()
    let phone = sdk.makeReplica()

    let change = try desktop.upsert(
        kind: .bookmark,
        entityID: "bookmark-1",
        payloadJSON: #"{"item_id":"book-1","char_offset":18}"#
    )
    try phone.importChange(change)

    let stateJSON = try phone.stateJSON()
    let state = try unwrap(
        JSONSerialization.jsonObject(with: Data(stateJSON.utf8))
            as? [String: Any]
    )
    let bookmarks = try unwrap(state["bookmarks"] as? [String: Any])
    return try unwrap(bookmarks["bookmark-1"] as? [String: Any])
}

private func unwrap<T>(_ value: T?) throws -> T {
    guard let value else {
        throw ReadioSDKTestError.missingValue
    }
    return value
}

private enum ReadioSDKTestError: Error {
    case missingValue
}
