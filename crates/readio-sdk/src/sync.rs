//! Platform-neutral durable state for Loro synchronization.
//!
//! Hosts persist this serializable snapshot/outbox value atomically before
//! making a network request. IndexedDB, SQLite, files, and HTTP remain platform
//! effects; update identity, replay, acknowledgement, and materialization stay
//! identical on every client.

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::bookmarks::Bookmark;
use crate::library::{LibraryItem, LibraryPage, LibraryQuery, query_library};
use crate::position::{Affinity, TextLocator};
use crate::replica::{ReadioReplica, ReplicaEntityKind, ReplicaError};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalReplicaEntity {
    #[serde(rename = "entityKind")]
    pub kind: ReplicaEntityKind,
    pub entity_id: String,
    pub payload: Value,
}

impl LocalReplicaEntity {
    pub fn new(kind: ReplicaEntityKind, entity_id: impl Into<String>, payload: Value) -> Self {
        Self {
            kind,
            entity_id: entity_id.into(),
            payload,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalReplicaEntityRef {
    #[serde(rename = "entityKind")]
    pub kind: ReplicaEntityKind,
    pub entity_id: String,
}

impl LocalReplicaEntityRef {
    pub fn new(kind: ReplicaEntityKind, entity_id: impl Into<String>) -> Self {
        Self {
            kind,
            entity_id: entity_id.into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingReplicaChange {
    pub change_id: String,
    pub change_base64: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReplicaReadingPosition {
    pub document_revision: String,
    pub char_offset: u64,
    pub affinity: Affinity,
    pub observed_at_unix_ms: i64,
    pub session_id: String,
    pub mutation_id: String,
    pub base_version: u64,
    pub version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<TextLocator>,
}

impl ReplicaReadingPosition {
    fn from_mutation(mutation: &ReadingPositionMutation, version: u64) -> Self {
        Self {
            document_revision: mutation.document_revision.clone(),
            char_offset: mutation.char_offset,
            affinity: mutation.affinity,
            observed_at_unix_ms: mutation.observed_at_unix_ms,
            session_id: mutation.session_id.clone(),
            mutation_id: mutation.mutation_id.clone(),
            base_version: mutation.base_version,
            version,
            locator: mutation.locator.clone(),
        }
    }

    fn matches(&self, mutation: &ReadingPositionMutation) -> bool {
        self.document_revision == mutation.document_revision
            && self.char_offset == mutation.char_offset
            && self.affinity == mutation.affinity
            && self.observed_at_unix_ms == mutation.observed_at_unix_ms
            && self.session_id == mutation.session_id
            && self.base_version == mutation.base_version
            && self.locator == mutation.locator
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReadingPositionMutation {
    pub document_revision: String,
    pub char_offset: u64,
    pub affinity: Affinity,
    pub observed_at_unix_ms: i64,
    pub session_id: String,
    pub mutation_id: String,
    pub base_version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<TextLocator>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionMutationStatus {
    Applied,
    Conflict,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionCandidateOrigin {
    Current,
    Incoming,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PositionCandidate {
    pub origin: PositionCandidateOrigin,
    #[serde(flatten)]
    pub position: ReplicaReadingPosition,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PositionMutationResponse {
    pub status: PositionMutationStatus,
    pub idempotent_replay: bool,
    pub version: u64,
    pub position: Option<ReplicaReadingPosition>,
    pub conflict_id: Option<String>,
    pub candidates: Vec<PositionCandidate>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ReplicaSyncState {
    #[serde(default)]
    snapshot_base64: String,
    #[serde(default)]
    outbox: Vec<PendingReplicaChange>,
}

impl ReplicaSyncState {
    pub fn apply(&mut self, event: ReplicaSyncEvent) -> Result<(), ReplicaSyncError> {
        match event {
            ReplicaSyncEvent::StageLocal { entities } => self.stage_local(&entities),
            ReplicaSyncEvent::DeleteLocal { entities } => self.delete_local(&entities),
            ReplicaSyncEvent::ImportRemote { changes } => self.import_remote(&changes),
            ReplicaSyncEvent::Acknowledge { change_ids } => {
                self.acknowledge(&change_ids);
                Ok(())
            }
        }
    }

    pub fn view(&self) -> Result<ReplicaSyncView, ReplicaSyncError> {
        Ok(ReplicaSyncView {
            state: self.materialized_state()?,
            outbox: self.outbox.clone(),
            snapshot_base64: self.snapshot_base64.clone(),
            heads_base64: self.heads_base64()?,
        })
    }

    pub fn stage_local(&mut self, entities: &[LocalReplicaEntity]) -> Result<(), ReplicaSyncError> {
        let replica = self.replica()?;

        for entity in entities {
            if replica.entity(entity.kind, &entity.entity_id).as_ref() == Some(&entity.payload) {
                continue;
            }

            let change = replica.upsert(entity.kind, &entity.entity_id, entity.payload.clone())?;
            self.outbox.push(PendingReplicaChange {
                change_id: format!("{:x}", Sha256::digest(&change)),
                change_base64: STANDARD.encode(change),
            });
        }

        self.snapshot_base64 = STANDARD.encode(replica.snapshot()?);
        Ok(())
    }

    pub fn delete_local(
        &mut self,
        entities: &[LocalReplicaEntityRef],
    ) -> Result<(), ReplicaSyncError> {
        let replica = self.replica()?;

        for entity in entities {
            if replica.entity(entity.kind, &entity.entity_id).is_none() {
                continue;
            }

            let change = replica.delete(entity.kind, &entity.entity_id)?;
            self.outbox.push(PendingReplicaChange {
                change_id: format!("{:x}", Sha256::digest(&change)),
                change_base64: STANDARD.encode(change),
            });
        }

        self.snapshot_base64 = STANDARD.encode(replica.snapshot()?);
        Ok(())
    }

    pub fn import_remote(
        &mut self,
        changes: &[PendingReplicaChange],
    ) -> Result<(), ReplicaSyncError> {
        let replica = self.replica()?;
        for change in changes {
            let bytes = STANDARD
                .decode(&change.change_base64)
                .map_err(ReplicaSyncError::base64)?;
            replica.import(&bytes)?;
        }
        self.snapshot_base64 = STANDARD.encode(replica.snapshot()?);
        Ok(())
    }

    pub fn acknowledge(&mut self, accepted_change_ids: &[String]) {
        self.outbox
            .retain(|change| !accepted_change_ids.contains(&change.change_id));
    }

    pub fn outbox(&self) -> &[PendingReplicaChange] {
        &self.outbox
    }

    pub fn snapshot_base64(&self) -> &str {
        &self.snapshot_base64
    }

    pub fn snapshot_bytes(&self) -> Result<Vec<u8>, ReplicaSyncError> {
        STANDARD
            .decode(&self.snapshot_base64)
            .map_err(ReplicaSyncError::base64)
    }

    pub fn heads_base64(&self) -> Result<String, ReplicaSyncError> {
        Ok(STANDARD.encode(self.heads()?))
    }

    pub fn heads(&self) -> Result<Vec<u8>, ReplicaSyncError> {
        Ok(self.replica()?.heads())
    }

    pub fn materialized_state(&self) -> Result<Value, ReplicaSyncError> {
        Ok(self.replica()?.state())
    }

    pub fn bookmarks_for_item(&self, item_id: &str) -> Result<Vec<Bookmark>, ReplicaSyncError> {
        let state = self.materialized_state()?;
        let mut bookmarks = state
            .get("bookmarks")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|entries| entries.values())
            .map(|payload| {
                serde_json::from_value::<Bookmark>(payload.clone())
                    .map_err(ReplicaSyncError::projection)
            })
            .collect::<Result<Vec<_>, _>>()?;
        bookmarks.retain(|bookmark| bookmark.item_id == item_id);
        bookmarks.sort_by(|left, right| {
            left.char_offset
                .cmp(&right.char_offset)
                .then_with(|| left.created_at_unix_ms.cmp(&right.created_at_unix_ms))
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(bookmarks)
    }

    pub fn settings(&self) -> Result<BTreeMap<String, String>, ReplicaSyncError> {
        self.materialized_state()?
            .get("settings")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|entries| entries.iter())
            .map(|(key, payload)| {
                let payload = serde_json::from_value::<SettingPayload>(payload.clone())
                    .map_err(ReplicaSyncError::projection)?;
                Ok((key.clone(), payload.value))
            })
            .collect()
    }

    pub fn query_library(&self, query: &LibraryQuery) -> Result<LibraryPage, ReplicaSyncError> {
        query_library(&self.library_items()?, query).map_err(ReplicaSyncError::projection)
    }

    pub fn library_item(&self, item_id: &str) -> Result<Option<LibraryItem>, ReplicaSyncError> {
        self.materialized_state()?
            .get("library_items")
            .and_then(Value::as_object)
            .and_then(|items| items.get(item_id))
            .cloned()
            .map(|payload| serde_json::from_value(payload).map_err(ReplicaSyncError::projection))
            .transpose()
    }

    pub fn reading_position(
        &self,
        item_id: &str,
    ) -> Result<Option<ReplicaReadingPosition>, ReplicaSyncError> {
        self.materialized_state()?
            .get("reading_positions")
            .and_then(Value::as_object)
            .and_then(|positions| positions.get(item_id))
            .cloned()
            .map(|payload| serde_json::from_value(payload).map_err(ReplicaSyncError::projection))
            .transpose()
    }

    pub fn save_reading_position(
        &mut self,
        item_id: &str,
        mutation: &ReadingPositionMutation,
    ) -> Result<PositionMutationResponse, ReplicaSyncError> {
        let current = self.reading_position(item_id)?;
        if let Some(position) = current
            .as_ref()
            .filter(|position| position.mutation_id == mutation.mutation_id)
        {
            if !position.matches(mutation) {
                return Err(ReplicaSyncError::MutationIdReuse(
                    mutation.mutation_id.clone(),
                ));
            }
            return Ok(PositionMutationResponse {
                status: PositionMutationStatus::Applied,
                idempotent_replay: true,
                version: position.version,
                position: Some(position.clone()),
                conflict_id: None,
                candidates: Vec::new(),
            });
        }

        let current_version = current.as_ref().map_or(0, |position| position.version);
        if mutation.base_version != current_version {
            let mut candidates = current
                .iter()
                .cloned()
                .map(|position| PositionCandidate {
                    origin: PositionCandidateOrigin::Current,
                    position,
                })
                .collect::<Vec<_>>();
            candidates.push(PositionCandidate {
                origin: PositionCandidateOrigin::Incoming,
                position: ReplicaReadingPosition::from_mutation(mutation, mutation.base_version),
            });
            return Ok(PositionMutationResponse {
                status: PositionMutationStatus::Conflict,
                idempotent_replay: false,
                version: current_version,
                position: current,
                conflict_id: Some(format!(
                    "position-conflict:{item_id}:{}",
                    mutation.mutation_id
                )),
                candidates,
            });
        }

        let version = current_version
            .checked_add(1)
            .ok_or(ReplicaSyncError::VersionOverflow)?;
        let position = ReplicaReadingPosition::from_mutation(mutation, version);
        self.stage_local(&[LocalReplicaEntity::new(
            ReplicaEntityKind::ReadingPosition,
            item_id,
            serde_json::to_value(&position).map_err(ReplicaSyncError::projection)?,
        )])?;
        Ok(PositionMutationResponse {
            status: PositionMutationStatus::Applied,
            idempotent_replay: false,
            version,
            position: Some(position),
            conflict_id: None,
            candidates: Vec::new(),
        })
    }

    fn library_items(&self) -> Result<Vec<LibraryItem>, ReplicaSyncError> {
        self.materialized_state()?
            .get("library_items")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|items| items.values())
            .map(|payload| {
                serde_json::from_value(payload.clone()).map_err(ReplicaSyncError::projection)
            })
            .collect()
    }

    fn replica(&self) -> Result<ReadioReplica, ReplicaSyncError> {
        if self.snapshot_base64.is_empty() {
            return Ok(ReadioReplica::new());
        }

        let snapshot = STANDARD
            .decode(&self.snapshot_base64)
            .map_err(ReplicaSyncError::base64)?;
        Ok(ReadioReplica::from_snapshot(&snapshot)?)
    }
}

#[derive(Deserialize)]
struct SettingPayload {
    value: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReplicaSyncEvent {
    StageLocal {
        entities: Vec<LocalReplicaEntity>,
    },
    DeleteLocal {
        entities: Vec<LocalReplicaEntityRef>,
    },
    ImportRemote {
        changes: Vec<PendingReplicaChange>,
    },
    Acknowledge {
        change_ids: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaSyncView {
    pub state: Value,
    pub outbox: Vec<PendingReplicaChange>,
    pub snapshot_base64: String,
    pub heads_base64: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReplicaSyncError {
    Base64(String),
    Projection(String),
    MutationIdReuse(String),
    VersionOverflow,
    Replica(ReplicaError),
}

impl ReplicaSyncError {
    fn base64(error: base64::DecodeError) -> Self {
        Self::Base64(error.to_string())
    }

    fn projection(error: impl Display) -> Self {
        Self::Projection(error.to_string())
    }
}

impl From<ReplicaError> for ReplicaSyncError {
    fn from(error: ReplicaError) -> Self {
        Self::Replica(error)
    }
}

impl Display for ReplicaSyncError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Base64(message) => write!(formatter, "invalid replica Base64: {message}"),
            Self::Projection(message) => {
                write!(
                    formatter,
                    "invalid replica materialized projection: {message}"
                )
            }
            Self::MutationIdReuse(mutation_id) => {
                write!(
                    formatter,
                    "reading position mutation id was reused: {mutation_id}"
                )
            }
            Self::VersionOverflow => formatter.write_str("reading position version overflow"),
            Self::Replica(error) => Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for ReplicaSyncError {}
