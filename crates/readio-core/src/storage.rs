use std::collections::HashMap;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_SYNC_BATCH_MUTATIONS: usize = 100;
const MAX_ID_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    LibraryItem,
    ReadingPosition,
    Bookmark,
    Setting,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationOperation {
    Upsert,
    Delete,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Mutation {
    pub mutation_id: String,
    pub entity_kind: EntityKind,
    pub entity_id: String,
    pub operation: MutationOperation,
    pub base_version: u64,
    pub client_timestamp_unix_ms: u64,
    pub payload: Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SyncPushBatch {
    pub device_id: String,
    pub mutations: Vec<Mutation>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StoredEntity {
    pub entity_kind: EntityKind,
    pub entity_id: String,
    pub operation: MutationOperation,
    pub version: u64,
    pub cursor: u64,
    pub server_timestamp_unix_ms: u64,
    pub payload: Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StoredMutation {
    pub mutation_id: String,
    pub entity_kind: EntityKind,
    pub entity_id: String,
    pub version: u64,
    pub cursor: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct StorageSnapshot {
    pub entities: Vec<StoredEntity>,
    pub mutations: Vec<StoredMutation>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationResultStatus {
    Applied,
    Replayed,
    Conflict,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MutationResult {
    pub mutation_id: String,
    pub status: MutationResultStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<StoredEntity>,
}

/// One host write with an optimistic precondition. Platform adapters may use a
/// local SQLite transaction, an atomic file replace, or a conditional D1
/// statement; none of those choices are allowed to change the transition.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StorageWrite {
    pub device_id: String,
    pub mutation_id: String,
    pub expected_version: u64,
    pub record: StoredEntity,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StoragePlan {
    pub cursor: u64,
    pub results: Vec<MutationResult>,
    pub writes: Vec<StorageWrite>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageError {
    InvalidId { field: &'static str },
    BatchTooLarge { actual: usize, limit: usize },
    MutationIdCollision { mutation_id: String },
    CursorOverflow,
    VersionOverflow,
}

impl Display for StorageError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidId { field } => {
                write!(formatter, "{field} must be 1..={MAX_ID_BYTES} bytes")
            }
            Self::BatchTooLarge { actual, limit } => {
                write!(
                    formatter,
                    "sync batch has {actual} mutations; limit is {limit}"
                )
            }
            Self::MutationIdCollision { mutation_id } => {
                write!(
                    formatter,
                    "mutation id {mutation_id} was reused for another entity"
                )
            }
            Self::CursorOverflow => formatter.write_str("sync cursor overflow"),
            Self::VersionOverflow => formatter.write_str("entity version overflow"),
        }
    }
}

impl std::error::Error for StorageError {}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_BYTES
}

/// Computes the complete deterministic storage transition before any host I/O.
/// The host must execute each write with `expected_version` as a compare-and-
/// swap precondition. A failed precondition means another request won the race;
/// the adapter reloads the snapshot and asks Core for a fresh plan.
pub fn plan_sync_push(
    snapshot: &StorageSnapshot,
    batch: &SyncPushBatch,
    server_timestamp_unix_ms: u64,
    starting_cursor: u64,
) -> Result<StoragePlan, StorageError> {
    if !valid_id(&batch.device_id) {
        return Err(StorageError::InvalidId { field: "device_id" });
    }
    if batch.mutations.len() > MAX_SYNC_BATCH_MUTATIONS {
        return Err(StorageError::BatchTooLarge {
            actual: batch.mutations.len(),
            limit: MAX_SYNC_BATCH_MUTATIONS,
        });
    }

    let mut entities: HashMap<(EntityKind, String), StoredEntity> = snapshot
        .entities
        .iter()
        .cloned()
        .map(|record| ((record.entity_kind, record.entity_id.clone()), record))
        .collect();
    let mut seen: HashMap<String, StoredMutation> = snapshot
        .mutations
        .iter()
        .cloned()
        .map(|mutation| (mutation.mutation_id.clone(), mutation))
        .collect();
    let mut cursor = starting_cursor;
    let mut results = Vec::with_capacity(batch.mutations.len());
    let mut writes = Vec::with_capacity(batch.mutations.len());

    for mutation in &batch.mutations {
        if !valid_id(&mutation.mutation_id) {
            return Err(StorageError::InvalidId {
                field: "mutation_id",
            });
        }
        if !valid_id(&mutation.entity_id) {
            return Err(StorageError::InvalidId { field: "entity_id" });
        }

        if let Some(recorded) = seen.get(&mutation.mutation_id) {
            if recorded.entity_kind != mutation.entity_kind
                || recorded.entity_id != mutation.entity_id
            {
                return Err(StorageError::MutationIdCollision {
                    mutation_id: mutation.mutation_id.clone(),
                });
            }
            results.push(MutationResult {
                mutation_id: mutation.mutation_id.clone(),
                status: MutationResultStatus::Replayed,
                version: Some(recorded.version),
                cursor: Some(recorded.cursor),
                current: None,
            });
            continue;
        }

        let key = (mutation.entity_kind, mutation.entity_id.clone());
        let current = entities.get(&key).cloned();
        let current_version = current.as_ref().map_or(0, |record| record.version);
        if mutation.base_version != current_version {
            results.push(MutationResult {
                mutation_id: mutation.mutation_id.clone(),
                status: MutationResultStatus::Conflict,
                version: None,
                cursor: None,
                current,
            });
            continue;
        }

        let version = current_version
            .checked_add(1)
            .ok_or(StorageError::VersionOverflow)?;
        cursor = cursor.checked_add(1).ok_or(StorageError::CursorOverflow)?;
        let record = StoredEntity {
            entity_kind: mutation.entity_kind,
            entity_id: mutation.entity_id.clone(),
            operation: mutation.operation,
            version,
            cursor,
            server_timestamp_unix_ms,
            payload: mutation.payload.clone(),
        };
        let recorded = StoredMutation {
            mutation_id: mutation.mutation_id.clone(),
            entity_kind: mutation.entity_kind,
            entity_id: mutation.entity_id.clone(),
            version,
            cursor,
        };

        writes.push(StorageWrite {
            device_id: batch.device_id.clone(),
            mutation_id: mutation.mutation_id.clone(),
            expected_version: current_version,
            record: record.clone(),
        });
        entities.insert(key, record);
        seen.insert(mutation.mutation_id.clone(), recorded);
        results.push(MutationResult {
            mutation_id: mutation.mutation_id.clone(),
            status: MutationResultStatus::Applied,
            version: Some(version),
            cursor: Some(cursor),
            current: None,
        });
    }

    Ok(StoragePlan {
        cursor,
        results,
        writes,
    })
}
