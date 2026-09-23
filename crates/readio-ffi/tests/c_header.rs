use std::{env, fs, process::Command};

#[test]
fn public_header_exposes_the_supported_c_abi() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path");
    let include_dir = format!("{manifest_dir}/include");
    let source_path = env::temp_dir().join(format!("readio-ffi-header-{}.c", std::process::id()));
    let source = r#"
#include <stdint.h>
#include "readio.h"

static ReadioFfiResult (*create_state)(uint32_t) = readio_create_playback_state;
static ReadioFfiResult (*reduce_state)(const char *, const char *, const char *) =
    readio_reduce_playback;
static ReadioFfiResult (*create_text_state)(uint32_t) = readio_create_text_advance_state;
static ReadioFfiResult (*builtin_sample)(const char *) = readio_builtin_sample;
static ReadioFfiResult (*content_identity)(const uint8_t *, size_t) = readio_content_identity;
static ReadioFfiResult (*plan_import)(const char *) = readio_plan_import;
static ReadioFfiResult (*plan_forget)(const char *) = readio_plan_forget;
static ReadioFfiResult (*reduce_text_state)(const char *, const char *) =
    readio_reduce_text_advance;
static ReadioFfiResult (*parse_html)(const char *, const char *) = readio_parse_html;
static ReadioFfiResult (*parse_html_reader)(const char *, const char *) = readio_parse_html_reader;
static ReadioFfiResult (*parse_markdown_reader)(const char *, const char *) =
    readio_parse_markdown_reader;
static ReadioFfiResult (*parse_plain_text_reader)(const char *, const char *) =
    readio_parse_plain_text_reader;
static ReadioFfiResult (*parse_pdf)(const uint8_t *, size_t) = readio_parse_pdf;
static ReadioFfiResult (*parse_epub)(const uint8_t *, size_t) = readio_parse_epub;
static ReadioFfiBytesResult (*extract_epub_asset)(const uint8_t *, size_t, const char *) =
    readio_extract_epub_asset;
static ReadioFfiResult (*project_offset)(const char *, const char *) = readio_project_offset;
static ReadioFfiResult (*create_text_locator)(const char *, const char *) = readio_create_text_locator;
static ReadioFfiResult (*restore_text_locator)(const char *, const char *) = readio_restore_text_locator;
static ReadioFfiResult (*index_document_text)(const char *) = readio_index_document_text;
static ReadioFfiResult (*resolve_highlight)(const char *, const char *, const char *) =
    readio_resolve_highlight;
static ReadioFfiResult (*resolve_block_offset)(const char *, const char *) =
    readio_resolve_block_offset;
static ReadioFfiResult (*restore_position)(const char *, const char *) = readio_restore_position;
static ReadioFfiResult (*search_document)(const char *, const char *) = readio_search_document;
static ReadioFfiResult (*upsert_bookmark)(const char *, const char *, const char *) =
    readio_upsert_bookmark;
static ReadioFfiResult (*plan_sync_push)(const char *, const char *, uint64_t, uint64_t) =
    readio_plan_sync_push;
static ReadioFfiResult (*sync_reduce)(const char *, const char *) = readio_sync_reduce;
static ReadioFfiResult (*sync_view)(const char *) = readio_sync_view;
static ReadioFfiResult (*sync_bookmarks)(const char *, const char *) = readio_sync_bookmarks;
static ReadioReplicaHandle *(*replica_new)(void) = readio_replica_new;
static ReadioFfiReplicaResult (*replica_from_snapshot)(const char *) =
    readio_replica_from_snapshot;
static ReadioFfiResult (*replica_upsert)(ReadioReplicaHandle *, const char *, const char *, const char *) =
    readio_replica_upsert;
static ReadioFfiResult (*replica_import)(ReadioReplicaHandle *, const char *) =
    readio_replica_import_change;
static ReadioFfiResult (*replica_snapshot)(ReadioReplicaHandle *) = readio_replica_snapshot;
static ReadioFfiResult (*replica_heads)(ReadioReplicaHandle *) = readio_replica_heads;
static ReadioFfiResult (*replica_state)(ReadioReplicaHandle *) = readio_replica_state;
static void (*replica_free)(ReadioReplicaHandle *) = readio_replica_free;
static void (*free_string)(char *) = readio_string_free;
static void (*free_bytes)(uint8_t *, size_t) = readio_bytes_free;

int main(void) {
    return create_state == 0 || reduce_state == 0 || create_text_state == 0 ||
           builtin_sample == 0 || content_identity == 0 || plan_import == 0 || plan_forget == 0 ||
           reduce_text_state == 0 || parse_html == 0 || parse_html_reader == 0 ||
           parse_markdown_reader == 0 || parse_plain_text_reader == 0 ||
           parse_pdf == 0 || parse_epub == 0 ||
           extract_epub_asset == 0 || project_offset == 0 || create_text_locator == 0 ||
           restore_text_locator == 0 || index_document_text == 0 || resolve_highlight == 0 ||
           resolve_block_offset == 0 || restore_position == 0 || search_document == 0 ||
           upsert_bookmark == 0 || plan_sync_push == 0 || sync_reduce == 0 || sync_view == 0 ||
           sync_bookmarks == 0 ||
           replica_new == 0 ||
           replica_from_snapshot == 0 || replica_upsert == 0 || replica_import == 0 ||
           replica_snapshot == 0 || replica_heads == 0 || replica_state == 0 ||
           replica_free == 0 || free_string == 0 || free_bytes == 0;
}
"#;
    fs::write(&source_path, source).expect("the C fixture can be written");

    let compiler = env::var("CC").unwrap_or_else(|_| "cc".to_owned());
    let output = Command::new(compiler)
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-fsyntax-only"])
        .arg("-I")
        .arg(include_dir)
        .arg(&source_path)
        .output()
        .expect("a C compiler is required to verify the public header");

    let _ = fs::remove_file(source_path);
    assert!(
        output.status.success(),
        "the public C header must compile for consumers:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn apple_module_map_imports_the_c_abi_as_readio_core() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path");
    let include_dir = format!("{manifest_dir}/include");
    let source_path = env::temp_dir().join(format!("readio-ffi-module-{}.m", std::process::id()));
    let module_cache =
        env::temp_dir().join(format!("readio-ffi-module-cache-{}", std::process::id()));
    let source = r#"
@import ReadioCore;

static ReadioFfiResult (*create_state)(uint32_t) = readio_create_playback_state;

int main(void) {
    return create_state == 0;
}
"#;
    fs::write(&source_path, source).expect("the Objective-C fixture can be written");

    let compiler = env::var("CC").unwrap_or_else(|_| "cc".to_owned());
    let output = Command::new(compiler)
        .args(["-x", "objective-c", "-fmodules", "-fsyntax-only"])
        .arg(format!("-fmodules-cache-path={}", module_cache.display()))
        .arg("-I")
        .arg(include_dir)
        .arg(&source_path)
        .output()
        .expect("a Clang-compatible compiler is required to verify the Apple module");

    let _ = fs::remove_file(source_path);
    let _ = fs::remove_dir_all(module_cache);
    assert!(
        output.status.success(),
        "the Apple module map must expose ReadioCore:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
