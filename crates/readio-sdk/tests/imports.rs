use readio_sdk::{
    ReadioSdk,
    imports::{ImportOwnership, ImportRequest},
};

#[test]
fn sdk_exposes_portable_import_identity_and_planning() {
    let sdk = ReadioSdk::new();
    let identity = sdk.imports.content_identity(b"portable book");
    let plan = sdk.imports.plan(ImportRequest {
        content_identity: identity.clone(),
        ownership: ImportOwnership::Move,
        source_key: "file:///book.txt".to_string(),
        owned_storage_key: "content/book.txt".to_string(),
        replace_ownership: false,
        existing: None,
    });

    assert_eq!(plan.record.item_id, identity.digest);
    assert_eq!(plan.effects.len(), 2);
    assert_eq!(sdk.imports.forget(&plan.record).len(), 1);
}
