use readio_core::storage::{
    EntityKind, Mutation, MutationOperation, MutationResultStatus, StorageSnapshot, StoredEntity,
    StoredMutation, SyncPushBatch, plan_sync_push,
};
use serde_json::json;

fn mutation(id: &str, base_version: u64, payload: serde_json::Value) -> Mutation {
    Mutation {
        mutation_id: id.to_string(),
        entity_kind: EntityKind::ReadingPosition,
        entity_id: "book-1".to_string(),
        operation: MutationOperation::Upsert,
        base_version,
        client_timestamp_unix_ms: 100,
        payload,
    }
}

fn batch(mutations: Vec<Mutation>) -> SyncPushBatch {
    SyncPushBatch {
        device_id: "mac-a".to_string(),
        mutations,
    }
}

#[test]
fn a_new_local_mutation_becomes_one_conditional_storage_write() {
    let plan = plan_sync_push(
        &StorageSnapshot::default(),
        &batch(vec![mutation("m-1", 0, json!({ "char_offset": 42 }))]),
        1_000,
        0,
    )
    .expect("valid mutation");

    assert_eq!(plan.cursor, 1);
    assert_eq!(plan.results[0].status, MutationResultStatus::Applied);
    assert_eq!(plan.results[0].version, Some(1));
    assert_eq!(plan.writes.len(), 1);
    assert_eq!(plan.writes[0].expected_version, 0);
    assert_eq!(plan.writes[0].record.version, 1);
    assert_eq!(plan.writes[0].record.cursor, 1);
}

#[test]
fn retrying_a_recorded_mutation_is_an_idempotent_replay() {
    let snapshot = StorageSnapshot {
        entities: vec![StoredEntity {
            entity_kind: EntityKind::ReadingPosition,
            entity_id: "book-1".to_string(),
            operation: MutationOperation::Upsert,
            version: 1,
            cursor: 9,
            server_timestamp_unix_ms: 900,
            payload: json!({ "char_offset": 42 }),
        }],
        mutations: vec![StoredMutation {
            mutation_id: "m-1".to_string(),
            entity_kind: EntityKind::ReadingPosition,
            entity_id: "book-1".to_string(),
            version: 1,
            cursor: 9,
        }],
    };

    let plan = plan_sync_push(
        &snapshot,
        &batch(vec![mutation("m-1", 0, json!({ "char_offset": 42 }))]),
        1_000,
        9,
    )
    .expect("replay");

    assert_eq!(plan.cursor, 9);
    assert_eq!(plan.results[0].status, MutationResultStatus::Replayed);
    assert_eq!(plan.results[0].version, Some(1));
    assert_eq!(plan.results[0].cursor, Some(9));
    assert!(plan.writes.is_empty());
}

#[test]
fn a_stale_base_version_returns_the_current_record_without_overwriting_it() {
    let current = StoredEntity {
        entity_kind: EntityKind::ReadingPosition,
        entity_id: "book-1".to_string(),
        operation: MutationOperation::Upsert,
        version: 3,
        cursor: 12,
        server_timestamp_unix_ms: 950,
        payload: json!({ "char_offset": 80 }),
    };
    let snapshot = StorageSnapshot {
        entities: vec![current.clone()],
        mutations: vec![],
    };

    let plan = plan_sync_push(
        &snapshot,
        &batch(vec![mutation("m-stale", 2, json!({ "char_offset": 60 }))]),
        1_000,
        12,
    )
    .expect("conflict is a protocol result");

    assert_eq!(plan.results[0].status, MutationResultStatus::Conflict);
    assert_eq!(plan.results[0].current, Some(current));
    assert!(plan.writes.is_empty());
    assert_eq!(plan.cursor, 12);
}

#[test]
fn mutations_for_one_entity_are_planned_in_batch_order() {
    let plan = plan_sync_push(
        &StorageSnapshot::default(),
        &batch(vec![
            mutation("m-1", 0, json!({ "char_offset": 10 })),
            mutation("m-2", 1, json!({ "char_offset": 20 })),
        ]),
        1_000,
        4,
    )
    .expect("dependent mutations");

    assert_eq!(plan.cursor, 6);
    assert_eq!(plan.writes[0].record.version, 1);
    assert_eq!(plan.writes[1].expected_version, 1);
    assert_eq!(plan.writes[1].record.version, 2);
    assert_eq!(plan.results[1].status, MutationResultStatus::Applied);
}

#[test]
fn unbounded_batches_and_invalid_ids_are_rejected_before_io() {
    let too_many = batch(
        (0..101)
            .map(|index| mutation(&format!("m-{index}"), 0, json!({})))
            .collect(),
    );
    assert!(plan_sync_push(&StorageSnapshot::default(), &too_many, 0, 0).is_err());

    let invalid_device = SyncPushBatch {
        device_id: "".to_string(),
        mutations: vec![],
    };
    assert!(plan_sync_push(&StorageSnapshot::default(), &invalid_device, 0, 0).is_err());
}
