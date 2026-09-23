import Foundation

/// Portable settings are local-first CRDT entities. Device-only boot settings
/// such as the cloud endpoint remain in UserDefaults.
final class SettingsService: Sendable {
    private let localStore: AppleLocalReplicaStore

    init(localStore: AppleLocalReplicaStore = .shared) {
        self.localStore = localStore
    }

    func fetchSettings() async throws -> [String: String] {
        let settingsJSON = try await localStore.settingsJSON()
        return try JSONDecoder().decode(
            [String: String].self,
            from: Data(settingsJSON.utf8)
        )
    }

    func patchSettings(data: [String: String]) async throws {
        let entities = data.map { key, value in
            [
                "entityKind": "setting",
                "entityId": key,
                "payload": ["value": value],
            ] as [String: Any]
        }
        let entitiesJSON = String(
            decoding: try JSONSerialization.data(withJSONObject: entities),
            as: UTF8.self
        )
        try await localStore.stageLocal(entitiesJSON: entitiesJSON)
    }
}
