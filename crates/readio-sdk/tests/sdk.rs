use readio_sdk::{
    ReadioSdk,
    documents::{BlockKind, HtmlParseOptions},
    highlight::HighlightCursor,
    playback::{PlaybackEffect, PlaybackEvent, PlaybackStatus},
    reading::{EffortLevel, ReadingMode, ReadingPreferences},
    replica::ReplicaEntityKind,
};
use serde_json::json;

#[test]
fn sdk_exposes_the_reserved_builtin_sample_to_every_adapter() {
    let sample = ReadioSdk::new().samples.resolve("zh-TW");

    assert_eq!(sample.item_id, "readio:sample:v1");
    assert_eq!(sample.locale, "zh");
    assert_eq!(sample.title, "注意力的形状");
    assert!(sample.is_builtin_sample);
}

#[test]
fn sdk_exposes_the_redacted_support_bundle_policy_to_every_adapter() {
    let request: readio_sdk::diagnostics::SupportBundleRequest = serde_json::from_value(json!({
        "generated_at_unix_ms": 100,
        "platform": "apple",
        "app_version": "0.2.0-beta.12",
        "events": [{
            "timestamp_unix_ms": 90,
            "operation_id": "op-shared",
            "component": "native-tts",
            "category": "tts",
            "name": "provider_failed",
            "fields": {
                "provider": "on-device",
                "reason_code": "voice_unavailable",
                "book_text": "Never export this sentence"
            }
        }]
    }))
    .expect("support bundle request");

    let bundle = ReadioSdk::new().diagnostics.build(request);

    assert_eq!(bundle.events[0].operation_id, "op-shared");
    assert_eq!(bundle.events[0].fields["provider"], json!("on-device"));
    assert!(!bundle.events[0].fields.contains_key("book_text"));
}

#[test]
fn one_sdk_facade_runs_document_and_playback_behavior() {
    let sdk = ReadioSdk::new();
    let document = sdk
        .documents
        .parse_html(
            "<h1>One core</h1><p>Shared by every product.</p>",
            &HtmlParseOptions::default(),
        )
        .expect("parse document");

    assert!(matches!(
        document.blocks[0].kind,
        BlockKind::Heading { level: 1 }
    ));
    assert_eq!(document.blocks[1].text, "Shared by every product.");

    let state = sdk.playback.create_state(2);
    let transition = sdk
        .playback
        .reduce(Default::default(), state, PlaybackEvent::PlayRequested);

    assert_eq!(transition.state.status, PlaybackStatus::Loading);
    assert_eq!(
        transition.effects,
        vec![
            PlaybackEffect::FetchAudio {
                sentence: 0,
                generation: 1,
                priority: readio_sdk::playback::FetchPriority::User,
            },
            PlaybackEffect::PrefetchAudio { sentences: vec![1] },
        ]
    );
}

#[test]
fn sdk_exposes_complete_html_reader_ir_to_platform_adapters() {
    let reader = ReadioSdk::new()
        .documents
        .parse_html_reader(
            "<h1>One source</h1><p>First. Second.</p>",
            &HtmlParseOptions::default(),
        )
        .expect("reader IR");

    assert_eq!(reader.document.blocks.len(), 2);
    assert_eq!(reader.text_index.sentences.len(), 3);
    assert_eq!(reader.navigation[0].title, "One source");
}

#[test]
fn sdk_exposes_complete_plain_text_reader_ir_to_platform_adapters() {
    let reader = ReadioSdk::new()
        .documents
        .parse_plain_text_reader("First. Second.\n\nThird.", &HtmlParseOptions::default())
        .expect("plain reader IR");

    assert_eq!(reader.document.blocks.len(), 2);
    assert_eq!(reader.text_index.sentences.len(), 3);
    assert_eq!(reader.chapters[0].sentence_range, [0, 3]);
}

#[test]
fn sdk_exposes_complete_markdown_reader_ir_to_platform_adapters() {
    let reader = ReadioSdk::new()
        .documents
        .parse_markdown_reader(
            "# One source\n\nA **shared** document.",
            &HtmlParseOptions::default(),
        )
        .expect("Markdown reader IR");

    assert!(matches!(
        reader.document.blocks[0].kind,
        BlockKind::Heading { level: 1 }
    ));
    assert_eq!(reader.document.blocks[1].inline_spans[0].range, [2, 8]);
    assert_eq!(reader.navigation[0].title, "One source");
}

#[test]
fn sdk_maps_the_shared_playback_cursor_to_one_renderer_range() {
    let sdk = ReadioSdk::new();
    let document = sdk
        .documents
        .parse_html(
            "<p>First sentence. Second bright sentence.</p>",
            &HtmlParseOptions::default(),
        )
        .expect("document");
    let index = sdk.documents.index_text(&document);

    let target = sdk
        .documents
        .resolve_highlight(
            &document,
            &index,
            HighlightCursor {
                document_revision: document.revision.revision_digest.clone(),
                sentence_index: 1,
                word_index: Some(1),
                word_progress_basis_points: None,
            },
        )
        .expect("highlight");

    assert_eq!(target.sentence_range, [16, 39]);
    assert_eq!(target.word_range, Some([23, 29]));
}

#[test]
fn sdk_exposes_one_reading_mode_and_effort_projection_to_every_adapter() {
    let pace = ReadioSdk::new().reading.resolve(ReadingPreferences {
        mode: ReadingMode::Auto,
        base_text_characters_per_second: 50.0,
        effort: EffortLevel::Medium,
        ..ReadingPreferences::default()
    });

    assert_eq!(pace.mode, ReadingMode::Auto);
    assert_eq!(pace.multiplier, 1.5);
    assert_eq!(pace.text_characters_per_second, 75.0);
    assert_eq!(pace.audio_playback_rate, 1.5);
    assert!(pace.advances_automatically);
    assert!(!pace.uses_audio);
}

#[test]
fn sdk_replicas_merge_concurrent_loro_changes_without_a_conflict_branch() {
    let sdk = ReadioSdk::new();
    let desktop = sdk.new_replica();
    let web = sdk.new_replica();

    let seed = desktop
        .upsert(
            ReplicaEntityKind::Setting,
            "theme",
            json!({ "value": "system" }),
        )
        .expect("seed theme");
    web.import(&seed).expect("import common ancestor");

    let desktop_change = desktop
        .upsert(
            ReplicaEntityKind::Bookmark,
            "bookmark-desktop",
            json!({ "item_id": "book-1", "char_offset": 10 }),
        )
        .expect("desktop bookmark");
    let web_change = web
        .upsert(
            ReplicaEntityKind::Bookmark,
            "bookmark-web",
            json!({ "item_id": "book-1", "char_offset": 20 }),
        )
        .expect("web bookmark");

    desktop.import(&web_change).expect("merge web change");
    web.import(&desktop_change).expect("merge desktop change");

    assert_eq!(desktop.state(), web.state());
    assert_eq!(
        desktop.entity(ReplicaEntityKind::Bookmark, "bookmark-desktop"),
        Some(json!({ "item_id": "book-1", "char_offset": 10 }))
    );
    assert_eq!(
        desktop.entity(ReplicaEntityKind::Bookmark, "bookmark-web"),
        Some(json!({ "item_id": "book-1", "char_offset": 20 }))
    );
}

#[test]
fn sdk_replica_snapshot_restores_state_and_keeps_deletions_mergeable() {
    let sdk = ReadioSdk::new();
    let original = sdk.new_replica();
    original
        .upsert(
            ReplicaEntityKind::LibraryItem,
            "book-1",
            json!({ "title": "Local first" }),
        )
        .expect("insert library item");
    let snapshot = original.snapshot().expect("snapshot replica");
    let restored =
        readio_sdk::replica::ReadioReplica::from_snapshot(&snapshot).expect("restore replica");

    assert_eq!(restored.state(), original.state());

    let deletion = restored
        .delete(ReplicaEntityKind::LibraryItem, "book-1")
        .expect("delete library item");
    original.import(&deletion).expect("merge deletion");

    assert_eq!(original.state(), restored.state());
    assert_eq!(
        original.entity(ReplicaEntityKind::LibraryItem, "book-1"),
        None
    );
}

#[test]
fn sdk_replica_heads_round_trip_with_its_snapshot() {
    let sdk = ReadioSdk::new();
    let original = sdk.new_replica();
    assert!(original.heads().is_empty());

    original
        .upsert(
            ReplicaEntityKind::Setting,
            "language",
            json!({ "value": "zh-CN" }),
        )
        .expect("update setting");
    let heads = original.heads();
    assert!(!heads.is_empty());

    let snapshot = original.snapshot().expect("snapshot replica");
    let restored =
        readio_sdk::replica::ReadioReplica::from_snapshot(&snapshot).expect("restore replica");
    assert_eq!(restored.heads(), heads);
}

#[test]
fn sdk_sync_state_persists_snapshot_outbox_and_remote_imports() {
    use readio_sdk::sync::{LocalReplicaEntity, ReplicaSyncState};

    let mut local = ReplicaSyncState::default();
    let entity = LocalReplicaEntity::new(
        ReplicaEntityKind::ReadingPosition,
        "book-1",
        json!({ "char_offset": 42 }),
    );
    local
        .stage_local(std::slice::from_ref(&entity))
        .expect("stage local entity");
    assert_eq!(local.outbox().len(), 1);

    let encoded = serde_json::to_string(&local).expect("encode sync state");
    let mut restored: ReplicaSyncState =
        serde_json::from_str(&encoded).expect("restore sync state");
    restored
        .stage_local(&[entity])
        .expect("unchanged entity stays clean");
    assert_eq!(restored.outbox().len(), 1);

    let mut remote = ReplicaSyncState::default();
    remote
        .import_remote(restored.outbox())
        .expect("import remote changes");
    assert!(remote.outbox().is_empty());
    assert_eq!(
        remote.materialized_state().expect("materialized state")["reading_positions"]["book-1"],
        json!({ "char_offset": 42 })
    );
}

#[test]
fn sdk_sync_state_replays_local_deletions_as_crdt_changes() {
    use readio_sdk::sync::{LocalReplicaEntity, LocalReplicaEntityRef, ReplicaSyncState};

    let entity = LocalReplicaEntity::new(
        ReplicaEntityKind::Bookmark,
        "book-1:bookmark-1",
        json!({
            "id": "bookmark-1",
            "item_id": "book-1",
            "char_offset": 42
        }),
    );
    let mut local = ReplicaSyncState::default();
    local
        .stage_local(&[entity])
        .expect("stage bookmark before deleting it");
    let create_changes = local.outbox().to_vec();
    let create_change_ids = create_changes
        .iter()
        .map(|change| change.change_id.clone())
        .collect::<Vec<_>>();
    local.acknowledge(&create_change_ids);

    local
        .delete_local(&[LocalReplicaEntityRef::new(
            ReplicaEntityKind::Bookmark,
            "book-1:bookmark-1",
        )])
        .expect("stage local bookmark deletion");

    assert_eq!(local.outbox().len(), 1);
    assert_eq!(
        local.materialized_state().expect("local state")["bookmarks"],
        json!({})
    );

    let mut remote = ReplicaSyncState::default();
    remote
        .import_remote(&create_changes)
        .expect("replay bookmark creation on another replica");
    remote
        .import_remote(local.outbox())
        .expect("replay deletion on another replica");

    assert_eq!(
        remote.materialized_state().expect("remote state")["bookmarks"],
        json!({})
    );
}

#[test]
fn sdk_sync_state_projects_bookmarks_in_portable_reader_order() {
    use readio_sdk::sync::{LocalReplicaEntity, ReplicaSyncState};

    let bookmark = |id: &str, item_id: &str, char_offset: u64| {
        LocalReplicaEntity::new(
            ReplicaEntityKind::Bookmark,
            id,
            json!({
                "id": id,
                "item_id": item_id,
                "char_offset": char_offset,
                "affinity": "forward",
                "note": id,
                "created_at_unix_ms": 10,
                "updated_at_unix_ms": 10,
                "version": 1
            }),
        )
    };
    let mut state = ReplicaSyncState::default();
    state
        .stage_local(&[
            bookmark("later", "book-1", 99),
            bookmark("other", "book-2", 1),
            bookmark("earlier", "book-1", 4),
        ])
        .expect("stage bookmark fixtures");

    let projected = state
        .bookmarks_for_item("book-1")
        .expect("project bookmark list");

    assert_eq!(
        projected
            .iter()
            .map(|bookmark| bookmark.id.as_str())
            .collect::<Vec<_>>(),
        vec!["earlier", "later"]
    );
}

#[test]
fn sdk_sync_state_projects_portable_settings_without_platform_defaults() {
    use readio_sdk::sync::{LocalReplicaEntity, ReplicaSyncState};

    let mut state = ReplicaSyncState::default();
    state
        .stage_local(&[
            LocalReplicaEntity::new(
                ReplicaEntityKind::Setting,
                "theme",
                json!({ "value": "dark" }),
            ),
            LocalReplicaEntity::new(
                ReplicaEntityKind::Setting,
                "font_size",
                json!({ "value": "20" }),
            ),
        ])
        .expect("stage portable settings");

    assert_eq!(
        state.settings().expect("project settings"),
        std::collections::BTreeMap::from([
            ("font_size".to_string(), "20".to_string()),
            ("theme".to_string(), "dark".to_string()),
        ])
    );
}

#[test]
fn sdk_sync_state_queries_materialized_library_items_through_core() {
    use readio_sdk::library::{LibraryQuery, LibrarySortBy, SortOrder};
    use readio_sdk::sync::{LocalReplicaEntity, ReplicaSyncState};

    let library_item = |id: &str, title: &str| {
        LocalReplicaEntity::new(
            ReplicaEntityKind::LibraryItem,
            id,
            json!({
                "id": id,
                "title": title,
                "type": "txt",
                "progress": 0,
                "date": "2026-01-01",
                "category": "imported",
                "folder_id": "f1",
                "source": format!("upload:{id}"),
                "file_path": format!("local:{id}"),
                "cover_image": null,
                "voice": null,
                "speed": null,
                "current_chapter": null,
                "content_format": null
            }),
        )
    };
    let mut state = ReplicaSyncState::default();
    state
        .stage_local(&[library_item("b", "Beta"), library_item("a", "Alpha")])
        .expect("stage library fixtures");

    let page = state
        .query_library(&LibraryQuery {
            sort_by: LibrarySortBy::Title,
            sort_order: SortOrder::Ascending,
            ..LibraryQuery::default()
        })
        .expect("query materialized library");

    assert_eq!(
        page.items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert_eq!(
        state
            .library_item("a")
            .expect("project library item")
            .expect("library item exists")
            .title,
        "Alpha"
    );
}

#[test]
fn sdk_sync_state_projects_the_versioned_reading_position() {
    use readio_sdk::sync::{LocalReplicaEntity, ReplicaSyncState};

    let mut state = ReplicaSyncState::default();
    state
        .stage_local(&[LocalReplicaEntity::new(
            ReplicaEntityKind::ReadingPosition,
            "book-1",
            json!({
                "document_revision": "revision-1",
                "char_offset": 44,
                "affinity": "forward",
                "observed_at_unix_ms": 1_700_000_000_000_i64,
                "session_id": "session-a",
                "mutation_id": "mutation-a",
                "base_version": 0,
                "version": 1
            }),
        )])
        .expect("stage reading position");

    let position = state
        .reading_position("book-1")
        .expect("project reading position")
        .expect("reading position exists");
    assert_eq!(position.char_offset, 44);
    assert_eq!(position.version, 1);
    assert_eq!(position.document_revision, "revision-1");
}

#[test]
fn sdk_sync_state_owns_reading_position_version_replay_and_conflict_semantics() {
    use readio_sdk::position::Affinity;
    use readio_sdk::sync::{PositionMutationStatus, ReadingPositionMutation, ReplicaSyncState};

    let sdk = ReadioSdk::new();
    let document = sdk
        .documents
        .parse_html("<p>Stable progress.</p>", &HtmlParseOptions::default())
        .expect("document");
    let locator = sdk
        .documents
        .create_text_locator(&document, 7, Affinity::Forward);
    let mut state = ReplicaSyncState::default();
    let mutation = ReadingPositionMutation {
        document_revision: "revision-1".to_string(),
        char_offset: 44,
        affinity: Affinity::Forward,
        observed_at_unix_ms: 1_700_000_000_000,
        session_id: "session-a".to_string(),
        mutation_id: "mutation-a".to_string(),
        base_version: 0,
        locator: Some(locator.clone()),
    };

    let applied = state
        .save_reading_position("book-1", &mutation)
        .expect("apply reading position");
    assert_eq!(applied.status, PositionMutationStatus::Applied);
    assert_eq!(applied.version, 1);
    assert_eq!(
        applied
            .position
            .as_ref()
            .and_then(|position| position.locator.as_ref()),
        Some(&locator)
    );
    assert!(!applied.idempotent_replay);
    assert_eq!(state.outbox().len(), 1);

    let replay = state
        .save_reading_position("book-1", &mutation)
        .expect("replay reading position");
    assert!(replay.idempotent_replay);
    assert_eq!(state.outbox().len(), 1);

    let conflict = state
        .save_reading_position(
            "book-1",
            &ReadingPositionMutation {
                char_offset: 4,
                mutation_id: "mutation-b".to_string(),
                ..mutation
            },
        )
        .expect("return conflict projection");
    assert_eq!(conflict.status, PositionMutationStatus::Conflict);
    assert_eq!(conflict.candidates.len(), 2);
    assert_eq!(state.outbox().len(), 1);
}
