use readio_core::{
    documents::ContentIdentity,
    imports::{
        ImportEffect, ImportOutcome, ImportOwnership, ImportRecord, ImportRequest,
        content_identity, plan_forget, plan_import,
    },
};
use serde_json::json;

fn request(bytes: &[u8], ownership: ImportOwnership) -> ImportRequest {
    ImportRequest {
        content_identity: content_identity(bytes),
        ownership,
        source_key: "file:///Users/me/book.epub".to_string(),
        owned_storage_key: "readio-content/book.epub".to_string(),
        replace_ownership: false,
        existing: None,
    }
}

#[test]
fn identity_hashes_all_bytes_and_is_path_independent() {
    let mut left = vec![b'a'; 256 * 1024];
    let mut right = left.clone();
    right[200 * 1024] = b'b';

    let first = content_identity(&left);
    let same = content_identity(&left);
    let changed_after_the_old_prefix_budget = content_identity(&right);

    assert_eq!(first, same);
    assert_ne!(first, changed_after_the_old_prefix_budget);
    assert_eq!(first.algorithm, "blake3-prefix-v1");
    assert_eq!(first.byte_length, left.len() as u64);

    left.clear();
    assert_ne!(content_identity(&left), first);
}

#[test]
fn copy_link_and_move_emit_only_the_host_effects_their_ownership_requires() {
    let copied = plan_import(request(b"same book", ImportOwnership::Copy));
    assert_eq!(copied.outcome, ImportOutcome::Created);
    assert_eq!(copied.record.item_id, copied.record.content_identity.digest);
    assert_eq!(copied.record.ownership, ImportOwnership::Copy);
    assert_eq!(copied.record.storage_key, "readio-content/book.epub");
    assert_eq!(
        copied.effects,
        vec![ImportEffect::WriteOwnedContent {
            source_key: "file:///Users/me/book.epub".to_string(),
            storage_key: "readio-content/book.epub".to_string(),
        }]
    );

    let linked = plan_import(request(b"linked book", ImportOwnership::Link));
    assert_eq!(linked.record.storage_key, linked.record.source_key);
    assert!(linked.effects.is_empty());

    let moved = plan_import(request(b"moved book", ImportOwnership::Move));
    assert_eq!(
        moved.effects,
        vec![
            ImportEffect::WriteOwnedContent {
                source_key: "file:///Users/me/book.epub".to_string(),
                storage_key: "readio-content/book.epub".to_string(),
            },
            ImportEffect::DeleteSourceAfterCommit {
                source_key: "file:///Users/me/book.epub".to_string(),
            },
        ]
    );
}

#[test]
fn duplicate_content_reuses_the_existing_item_without_touching_either_file() {
    let identity = content_identity(b"same book");
    let existing = ImportRecord {
        item_id: "existing-item".to_string(),
        content_identity: identity.clone(),
        ownership: ImportOwnership::Link,
        source_key: "file:///first/book.epub".to_string(),
        storage_key: "file:///first/book.epub".to_string(),
    };
    let plan = plan_import(ImportRequest {
        content_identity: identity,
        ownership: ImportOwnership::Move,
        source_key: "file:///second/book.epub".to_string(),
        owned_storage_key: "readio-content/book.epub".to_string(),
        replace_ownership: false,
        existing: Some(existing.clone()),
    });

    assert_eq!(plan.outcome, ImportOutcome::Reused);
    assert_eq!(plan.record, existing);
    assert!(plan.effects.is_empty());
}

#[test]
fn explicit_ownership_replacement_keeps_the_item_id_and_plans_platform_effects() {
    let identity = content_identity(b"same book");
    let request: ImportRequest = serde_json::from_value(json!({
        "content_identity": identity,
        "ownership": "copy",
        "source_key": "file:///Users/me/book.epub",
        "owned_storage_key": "readio-content/book.epub",
        "replace_ownership": true,
        "existing": {
            "item_id": "legacy-progress-id",
            "content_identity": identity,
            "ownership": "link",
            "source_key": "file:///Users/me/book.epub",
            "storage_key": "file:///Users/me/book.epub"
        }
    }))
    .expect("import request");

    let plan = serde_json::to_value(plan_import(request)).expect("import plan");

    assert_eq!(plan["outcome"], "updated");
    assert_eq!(plan["record"]["item_id"], "legacy-progress-id");
    assert_eq!(plan["record"]["ownership"], "copy");
    assert_eq!(
        plan["effects"],
        json!([{
            "type": "write_owned_content",
            "source_key": "file:///Users/me/book.epub",
            "storage_key": "readio-content/book.epub"
        }])
    );
}

#[test]
fn forget_deletes_only_readio_owned_content() {
    let identity = ContentIdentity {
        algorithm: "blake3-prefix-v1".to_string(),
        byte_length: 4,
        digest: "digest".to_string(),
    };
    let linked = ImportRecord {
        item_id: "linked".to_string(),
        content_identity: identity.clone(),
        ownership: ImportOwnership::Link,
        source_key: "file:///Users/me/book.epub".to_string(),
        storage_key: "file:///Users/me/book.epub".to_string(),
    };
    assert!(plan_forget(&linked).is_empty());

    let copied = ImportRecord {
        item_id: "copied".to_string(),
        content_identity: identity,
        ownership: ImportOwnership::Copy,
        source_key: "file:///Users/me/book.epub".to_string(),
        storage_key: "readio-content/book.epub".to_string(),
    };
    assert_eq!(
        plan_forget(&copied),
        vec![ImportEffect::DeleteOwnedContent {
            storage_key: "readio-content/book.epub".to_string(),
        }]
    );
}
