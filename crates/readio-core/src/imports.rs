use serde::{Deserialize, Serialize};

use crate::documents::ContentIdentity;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportOwnership {
    Copy,
    Link,
    Move,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ImportRecord {
    pub item_id: String,
    pub content_identity: ContentIdentity,
    pub ownership: ImportOwnership,
    pub source_key: String,
    pub storage_key: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ImportRequest {
    pub content_identity: ContentIdentity,
    pub ownership: ImportOwnership,
    pub source_key: String,
    pub owned_storage_key: String,
    #[serde(default)]
    pub replace_ownership: bool,
    pub existing: Option<ImportRecord>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportOutcome {
    Created,
    Reused,
    Updated,
}

/// Ordered effects executed by the platform adapter. In particular, Move puts
/// source deletion after the owned write so a failed import never loses data.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImportEffect {
    WriteOwnedContent {
        source_key: String,
        storage_key: String,
    },
    DeleteSourceAfterCommit {
        source_key: String,
    },
    DeleteOwnedContent {
        storage_key: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ImportPlan {
    pub outcome: ImportOutcome,
    pub record: ImportRecord,
    pub effects: Vec<ImportEffect>,
}

/// Full-byte content identity shared by every host. The historical algorithm
/// identifier is retained because it is already part of the Document IR wire
/// contract; the implementation hashes all bytes, never only a prefix.
pub fn content_identity(bytes: &[u8]) -> ContentIdentity {
    ContentIdentity {
        algorithm: "blake3-prefix-v1".to_string(),
        byte_length: bytes.len() as u64,
        digest: blake3::hash(bytes).to_hex().to_string(),
    }
}

pub fn plan_import(mut request: ImportRequest) -> ImportPlan {
    if let Some(existing) = request
        .existing
        .take()
        .filter(|record| record.content_identity == request.content_identity)
    {
        if request.replace_ownership && existing.ownership != request.ownership {
            return plan_ownership_replacement(existing, request);
        }
        return ImportPlan {
            outcome: ImportOutcome::Reused,
            record: existing,
            effects: Vec::new(),
        };
    }

    let storage_key = match request.ownership {
        ImportOwnership::Link => request.source_key.clone(),
        ImportOwnership::Copy | ImportOwnership::Move => request.owned_storage_key.clone(),
    };
    let record = ImportRecord {
        item_id: request.content_identity.digest.clone(),
        content_identity: request.content_identity,
        ownership: request.ownership,
        source_key: request.source_key.clone(),
        storage_key,
    };
    let effects = match request.ownership {
        ImportOwnership::Link => Vec::new(),
        ImportOwnership::Copy => vec![ImportEffect::WriteOwnedContent {
            source_key: request.source_key,
            storage_key: request.owned_storage_key,
        }],
        ImportOwnership::Move => vec![
            ImportEffect::WriteOwnedContent {
                source_key: request.source_key.clone(),
                storage_key: request.owned_storage_key,
            },
            ImportEffect::DeleteSourceAfterCommit {
                source_key: request.source_key,
            },
        ],
    };

    ImportPlan {
        outcome: ImportOutcome::Created,
        record,
        effects,
    }
}

fn plan_ownership_replacement(existing: ImportRecord, request: ImportRequest) -> ImportPlan {
    let storage_key = match request.ownership {
        ImportOwnership::Link => request.source_key.clone(),
        ImportOwnership::Copy | ImportOwnership::Move => request.owned_storage_key.clone(),
    };
    let record = ImportRecord {
        item_id: existing.item_id,
        content_identity: request.content_identity,
        ownership: request.ownership,
        source_key: request.source_key.clone(),
        storage_key: storage_key.clone(),
    };
    let mut effects = match request.ownership {
        ImportOwnership::Link => Vec::new(),
        ImportOwnership::Copy => vec![ImportEffect::WriteOwnedContent {
            source_key: request.source_key.clone(),
            storage_key: request.owned_storage_key.clone(),
        }],
        ImportOwnership::Move => vec![
            ImportEffect::WriteOwnedContent {
                source_key: request.source_key.clone(),
                storage_key: request.owned_storage_key.clone(),
            },
            ImportEffect::DeleteSourceAfterCommit {
                source_key: request.source_key.clone(),
            },
        ],
    };
    if existing.ownership != ImportOwnership::Link
        && existing.storage_key != storage_key
        && existing.storage_key != request.source_key
    {
        effects.push(ImportEffect::DeleteOwnedContent {
            storage_key: existing.storage_key,
        });
    }
    ImportPlan {
        outcome: ImportOutcome::Updated,
        record,
        effects,
    }
}

pub fn plan_forget(record: &ImportRecord) -> Vec<ImportEffect> {
    match record.ownership {
        ImportOwnership::Link => Vec::new(),
        ImportOwnership::Copy | ImportOwnership::Move => {
            vec![ImportEffect::DeleteOwnedContent {
                storage_key: record.storage_key.clone(),
            }]
        }
    }
}
