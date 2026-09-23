use std::fmt::{Display, Formatter};

use loro::{ExportMode, LoroDoc, ToJson};
use serde_json::{Map, Value};

pub use readio_core::storage::EntityKind as ReplicaEntityKind;

const MAX_ENTITY_ID_BYTES: usize = 128;

/// An encoded Loro update. It is safe to persist in an outbox and import more
/// than once; Loro identifies operations and makes replay idempotent.
pub type ReplicaChange = Vec<u8>;

#[derive(Clone, Debug)]
pub struct ReadioReplica {
    document: LoroDoc,
}

impl Default for ReadioReplica {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadioReplica {
    pub fn new() -> Self {
        Self {
            document: LoroDoc::new(),
        }
    }

    /// Restores a complete local replica from a compact Loro snapshot.
    pub fn from_snapshot(snapshot: &[u8]) -> Result<Self, ReplicaError> {
        let replica = Self::new();
        replica.import(snapshot)?;
        Ok(replica)
    }

    /// Applies one product entity mutation and returns only the new CRDT update.
    /// The caller persists the returned bytes in its platform outbox atomically
    /// with the local snapshot/database projection.
    pub fn upsert(
        &self,
        kind: ReplicaEntityKind,
        entity_id: &str,
        payload: Value,
    ) -> Result<ReplicaChange, ReplicaError> {
        validate_entity_id(entity_id)?;
        let encoded = serde_json::to_string(&payload).map_err(ReplicaError::json)?;
        let before = self.document.oplog_vv();
        self.document
            .get_map(root_name(kind))
            .insert(entity_id, encoded)
            .map_err(ReplicaError::loro)?;
        self.document.commit();
        self.export_since(&before)
    }

    pub fn delete(
        &self,
        kind: ReplicaEntityKind,
        entity_id: &str,
    ) -> Result<ReplicaChange, ReplicaError> {
        validate_entity_id(entity_id)?;
        let before = self.document.oplog_vv();
        self.document
            .get_map(root_name(kind))
            .delete(entity_id)
            .map_err(ReplicaError::loro)?;
        self.document.commit();
        self.export_since(&before)
    }

    /// Imports a remote update or snapshot. Updates may arrive repeatedly or in
    /// dependency order later; Loro retains pending changes until they connect.
    pub fn import(&self, bytes: &[u8]) -> Result<(), ReplicaError> {
        self.document.import(bytes).map_err(ReplicaError::loro)?;
        Ok(())
    }

    pub fn entity(&self, kind: ReplicaEntityKind, entity_id: &str) -> Option<Value> {
        let value = self.document.get_map(root_name(kind)).get(entity_id)?;
        let encoded = value.into_value().ok()?.into_string().ok()?;
        serde_json::from_str(encoded.as_str()).ok()
    }

    /// Returns a platform-neutral materialized projection. D1, SQLite, and
    /// IndexedDB adapters can rebuild their query indexes from this value.
    pub fn state(&self) -> Value {
        let mut state = Map::new();
        for kind in ENTITY_KINDS {
            let root = root_name(kind);
            let raw = self.document.get_map(root).get_deep_value().to_json_value();
            let mut decoded = Map::new();
            if let Value::Object(entries) = raw {
                for (entity_id, value) in entries {
                    let Some(encoded) = value.as_str() else {
                        continue;
                    };
                    if let Ok(payload) = serde_json::from_str(encoded) {
                        decoded.insert(entity_id, payload);
                    }
                }
            }
            state.insert(root.to_string(), Value::Object(decoded));
        }
        Value::Object(state)
    }

    pub fn snapshot(&self) -> Result<Vec<u8>, ReplicaError> {
        self.document
            .export(ExportMode::Snapshot)
            .map_err(ReplicaError::loro)
    }

    /// Encodes the current Loro frontier for device acknowledgement metadata.
    /// An empty replica has no heads, so adapters can omit the field entirely.
    pub fn heads(&self) -> Vec<u8> {
        let heads = self.document.oplog_frontiers();
        if heads.is_empty() {
            Vec::new()
        } else {
            heads.encode()
        }
    }

    fn export_since(&self, version: &loro::VersionVector) -> Result<ReplicaChange, ReplicaError> {
        self.document
            .export(ExportMode::updates(version))
            .map_err(ReplicaError::loro)
    }
}

const ENTITY_KINDS: [ReplicaEntityKind; 4] = [
    ReplicaEntityKind::LibraryItem,
    ReplicaEntityKind::ReadingPosition,
    ReplicaEntityKind::Bookmark,
    ReplicaEntityKind::Setting,
];

fn root_name(kind: ReplicaEntityKind) -> &'static str {
    match kind {
        ReplicaEntityKind::LibraryItem => "library_items",
        ReplicaEntityKind::ReadingPosition => "reading_positions",
        ReplicaEntityKind::Bookmark => "bookmarks",
        ReplicaEntityKind::Setting => "settings",
    }
}

fn validate_entity_id(entity_id: &str) -> Result<(), ReplicaError> {
    if entity_id.is_empty() || entity_id.len() > MAX_ENTITY_ID_BYTES {
        return Err(ReplicaError::InvalidEntityId);
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReplicaError {
    InvalidEntityId,
    Json(String),
    Loro(String),
}

impl ReplicaError {
    fn json(error: serde_json::Error) -> Self {
        Self::Json(error.to_string())
    }

    fn loro(error: impl Display) -> Self {
        Self::Loro(error.to_string())
    }
}

impl Display for ReplicaError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidEntityId => formatter.write_str("entity id must be 1..=128 bytes"),
            Self::Json(message) => write!(formatter, "invalid replica JSON: {message}"),
            Self::Loro(message) => write!(formatter, "Loro replica error: {message}"),
        }
    }
}

impl std::error::Error for ReplicaError {}
