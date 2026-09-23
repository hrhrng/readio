use readio::replica_sync::{LocalReplicaEntity, ReplicaSyncState};
use readio_sdk::replica::ReplicaEntityKind;
use serde_json::json;

#[test]
fn replica_outbox_survives_restart_and_stages_unchanged_local_state_only_once() {
    let entities = vec![
        LocalReplicaEntity::new(
            ReplicaEntityKind::LibraryItem,
            "book-1",
            json!({ "title": "Local first" }),
        ),
        LocalReplicaEntity::new(
            ReplicaEntityKind::ReadingPosition,
            "book-1",
            json!({ "char_offset": 42 }),
        ),
    ];
    let mut state = ReplicaSyncState::default();

    state
        .stage_local(&entities)
        .expect("stage first local state");
    assert_eq!(state.outbox().len(), 2);
    assert!(!state.snapshot_base64().is_empty());
    assert!(!state.heads_base64().expect("replica heads").is_empty());

    let encoded = serde_json::to_string(&state).expect("encode sync state");
    let mut restored: ReplicaSyncState =
        serde_json::from_str(&encoded).expect("restore sync state");
    restored
        .stage_local(&entities)
        .expect("unchanged state stays clean");
    assert_eq!(restored.outbox().len(), 2);
    assert_eq!(
        restored.materialized_state().expect("materialized replica")["reading_positions"]["book-1"],
        json!({ "char_offset": 42 })
    );

    let accepted = restored
        .outbox()
        .iter()
        .map(|change| change.change_id.clone())
        .collect::<Vec<_>>();
    restored.acknowledge(&accepted);
    assert!(restored.outbox().is_empty());
}

#[test]
fn pulled_changes_materialize_without_entering_the_local_outbox() {
    let mut remote = ReplicaSyncState::default();
    remote
        .stage_local(&[LocalReplicaEntity::new(
            ReplicaEntityKind::Bookmark,
            "mark-1",
            json!({ "book_id": "book-1", "char_offset": 9 }),
        )])
        .expect("stage remote change");

    let mut local = ReplicaSyncState::default();
    local
        .import_remote(remote.outbox())
        .expect("import pulled changes");
    local
        .import_remote(remote.outbox())
        .expect("replayed pull is idempotent");

    assert!(local.outbox().is_empty());
    assert_eq!(
        local.materialized_state().expect("materialized replica")["bookmarks"]["mark-1"],
        json!({ "book_id": "book-1", "char_offset": 9 })
    );
    let encoded = serde_json::to_string(&local).expect("encode imported snapshot");
    let restored: ReplicaSyncState =
        serde_json::from_str(&encoded).expect("restore imported snapshot");
    assert_eq!(
        restored.materialized_state().expect("restored replica")["bookmarks"]["mark-1"],
        json!({ "book_id": "book-1", "char_offset": 9 })
    );
}
