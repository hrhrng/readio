#ifndef READIO_CORE_H
#define READIO_CORE_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ReadioFfiResult {
    char *value;
    char *error;
} ReadioFfiResult;

typedef struct ReadioFfiBytesResult {
    uint8_t *value;
    size_t length;
    char *error;
} ReadioFfiBytesResult;

typedef struct ReadioReplicaHandle ReadioReplicaHandle;

typedef struct ReadioFfiReplicaResult {
    ReadioReplicaHandle *value;
    char *error;
} ReadioFfiReplicaResult;

ReadioFfiResult readio_create_playback_state(uint32_t sentence_count);
ReadioFfiResult readio_reduce_playback(
    const char *policy_json,
    const char *state_json,
    const char *event_json
);
ReadioFfiResult readio_resolve_reading_pace(const char *preferences_json);
ReadioFfiResult readio_project_reading_progress(const char *request_json);
ReadioFfiResult readio_resolve_reading_seek(const char *request_json);
ReadioFfiResult readio_create_text_advance_state(uint32_t sentence_count);
ReadioFfiResult readio_builtin_sample(const char *language_tag);
ReadioFfiResult readio_build_support_bundle(const char *request_json);
ReadioFfiResult readio_content_identity(const uint8_t *bytes, size_t length);
ReadioFfiResult readio_plan_import(const char *request_json);
ReadioFfiResult readio_plan_forget(const char *record_json);
ReadioFfiResult readio_reduce_text_advance(
    const char *state_json,
    const char *event_json
);
ReadioFfiResult readio_parse_html(const char *html, const char *options_json);
ReadioFfiResult readio_parse_html_reader(const char *html, const char *options_json);
ReadioFfiResult readio_parse_markdown_reader(const char *markdown, const char *options_json);
ReadioFfiResult readio_parse_plain_text_reader(const char *text, const char *options_json);
ReadioFfiResult readio_parse_pdf(const uint8_t *bytes, size_t length);
ReadioFfiResult readio_parse_epub(const uint8_t *bytes, size_t length);
ReadioFfiBytesResult readio_extract_epub_asset(
    const uint8_t *bytes,
    size_t length,
    const char *asset_id
);
ReadioFfiResult readio_project_offset(const char *document_json, const char *request_json);
ReadioFfiResult readio_create_text_locator(const char *document_json, const char *request_json);
ReadioFfiResult readio_restore_text_locator(const char *document_json, const char *locator_json);
ReadioFfiResult readio_index_document_text(const char *document_json);
ReadioFfiResult readio_resolve_highlight(
    const char *document_json,
    const char *index_json,
    const char *cursor_json
);
ReadioFfiResult readio_resolve_block_offset(const char *document_json, const char *request_json);
ReadioFfiResult readio_restore_position(const char *document_json, const char *position_json);
ReadioFfiResult readio_search_document(const char *document_json, const char *request_json);
ReadioFfiResult readio_upsert_bookmark(
    const char *document_json,
    const char *bookmarks_json,
    const char *request_json
);
ReadioFfiResult readio_plan_sync_push(
    const char *snapshot_json,
    const char *batch_json,
    uint64_t server_timestamp_unix_ms,
    uint64_t starting_cursor
);
ReadioFfiResult readio_sync_reduce(const char *state_json, const char *event_json);
ReadioFfiResult readio_sync_view(const char *state_json);
ReadioFfiResult readio_sync_query_library(const char *state_json, const char *query_json);
ReadioFfiResult readio_sync_library_item(const char *state_json, const char *item_id);
ReadioFfiResult readio_sync_settings(const char *state_json);
ReadioFfiResult readio_sync_bookmarks(const char *state_json, const char *item_id);
ReadioFfiResult readio_sync_reading_position(const char *state_json, const char *item_id);
ReadioFfiResult readio_sync_save_reading_position(
    const char *state_json,
    const char *item_id,
    const char *mutation_json
);
ReadioReplicaHandle *readio_replica_new(void);
ReadioFfiReplicaResult readio_replica_from_snapshot(const char *snapshot_base64);
ReadioFfiResult readio_replica_upsert(
    ReadioReplicaHandle *replica,
    const char *entity_kind,
    const char *entity_id,
    const char *payload_json
);
ReadioFfiResult readio_replica_import_change(
    ReadioReplicaHandle *replica,
    const char *change_base64
);
ReadioFfiResult readio_replica_snapshot(ReadioReplicaHandle *replica);
ReadioFfiResult readio_replica_heads(ReadioReplicaHandle *replica);
ReadioFfiResult readio_replica_state(ReadioReplicaHandle *replica);
void readio_replica_free(ReadioReplicaHandle *replica);
void readio_bytes_free(uint8_t *pointer, size_t length);
void readio_string_free(char *pointer);

#ifdef __cplusplus
}
#endif

#endif
