//! Optional cloud capability for the local-first TUI.
//!
//! Nothing in the normal reader calls this module unless the reader explicitly
//! runs `readio cloud …`. Passwords never cross the terminal: login uses the
//! RFC 8628 device flow, and the resulting bearer token lives in a 0600 file.

use std::collections::BTreeMap;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use readio_sdk::diagnostics::DiagnosticCategory;
use readio_sdk::position::TextLocator;
use readio_sdk::storage::EntityKind;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::cli::CloudCommand;
use crate::config::{BookVoice, Config, EffortConfig, Images, Reading};
use crate::diagnostics::{DiagnosticLog, unix_time_milliseconds};
use crate::i18n::Lang;
use crate::library::{Entry, Library};
use crate::paths;
use crate::replica_sync::{LocalReplicaEntity, PendingReplicaChange, ReplicaSyncState};
use crate::store::{Mark, Progress, SpeechCheckpoint, Store};
use crate::voice::device::Output;

const CLIENT_ID: &str = "readio-tui";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

#[derive(Debug, Clone, Serialize)]
pub struct LocalSyncPayload {
    pub entity_kind: EntityKind,
    pub entity_id: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SyncedProgress {
    title: String,
    chapter: usize,
    para: usize,
    chars_read: u64,
    sessions: u32,
    updated: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    locator: Option<TextLocator>,
    speech: Option<SpeechCheckpoint>,
    marks: Vec<Mark>,
}

impl From<&Progress> for SyncedProgress {
    fn from(progress: &Progress) -> Self {
        Self {
            title: progress.title.clone(),
            chapter: progress.chapter,
            para: progress.para,
            chars_read: progress.chars_read,
            sessions: progress.sessions,
            updated: progress.updated,
            locator: progress.locator.clone(),
            speech: progress.speech.clone(),
            marks: progress.marks.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SyncedVoice {
    engine: String,
    name: String,
    language: String,
    params: String,
    prefetch: usize,
    output: Output,
    books: BTreeMap<String, BookVoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SyncedSettings {
    language: Lang,
    reading: Reading,
    effort: EffortConfig,
    images: Images,
    voice: SyncedVoice,
}

impl From<&Config> for SyncedSettings {
    fn from(config: &Config) -> Self {
        Self {
            language: config.language,
            reading: config.reading.clone(),
            effort: config.effort,
            images: config.images.clone(),
            voice: SyncedVoice {
                engine: config.voice.engine.clone(),
                name: config.voice.name.clone(),
                language: config.voice.language.clone(),
                params: config.voice.params.clone(),
                prefetch: config.voice.prefetch,
                output: config.voice.output.clone(),
                books: config.voice.books.clone(),
            },
        }
    }
}

/// Produces the only payloads the TUI is allowed to upload. Source/origin paths
/// and executable engine definitions are deliberately absent.
pub fn local_sync_payloads(
    library: &Library,
    store: &Store,
    config: &Config,
) -> Vec<LocalSyncPayload> {
    let mut payloads = Vec::with_capacity(library.entries.len() + store.books.len() + 1);
    for entry in &library.entries {
        payloads.push(LocalSyncPayload {
            entity_kind: EntityKind::LibraryItem,
            entity_id: entry.id.clone(),
            payload: safe_library_payload(entry),
        });
    }
    for (id, progress) in &store.books {
        payloads.push(LocalSyncPayload {
            entity_kind: EntityKind::ReadingPosition,
            entity_id: id.clone(),
            payload: serde_json::to_value(SyncedProgress::from(progress)).unwrap_or(Value::Null),
        });
    }
    payloads.push(LocalSyncPayload {
        entity_kind: EntityKind::Setting,
        entity_id: "preferences".to_string(),
        payload: serde_json::to_value(SyncedSettings::from(config)).unwrap_or(Value::Null),
    });
    payloads
}

fn safe_library_payload(entry: &Entry) -> Value {
    json!({
        "title": entry.title,
        "author": entry.author,
        "bytes": entry.bytes,
        "chars": entry.chars,
        "words": entry.words,
        "chapters": entry.chapters,
        "imported": entry.imported,
        "last_opened": entry.last_opened,
        "content_available_on_device": entry.available(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CloudState {
    server: String,
    access_token: String,
    device_id: String,
    #[serde(default)]
    cursor: u64,
    #[serde(default)]
    replica: ReplicaSyncState,
}

impl CloudState {
    fn load() -> Result<Self> {
        let path = paths::cloud_file();
        let raw = std::fs::read_to_string(&path)
            .with_context(|| "not signed in; run `readio cloud login`")?;
        serde_json::from_str(&raw).with_context(|| format!("cannot read {}", path.display()))
    }

    fn save(&self) -> Result<()> {
        let path = paths::cloud_file();
        paths::write_atomic(&path, &serde_json::to_string_pretty(self)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    expires_in: u64,
    interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct DeviceTokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct AuthError {
    error: Option<String>,
    code: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReplicaPushBatch {
    device_id: String,
    ack_cursor: u64,
    heads_base64: String,
    changes: Vec<PendingReplicaChange>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplicaPushResponse {
    cursor: u64,
    accepted_change_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplicaPullResponse {
    cursor: u64,
    has_more: bool,
    changes: Vec<ReplicaStoredChange>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplicaStoredChange {
    change_id: String,
    change_base64: String,
}

pub async fn run(command: CloudCommand) -> Result<()> {
    match command {
        CloudCommand::Login { server } => login(&server).await,
        CloudCommand::Logout => logout(),
        CloudCommand::Status => status().await,
        CloudCommand::Sync => sync().await,
    }
}

async fn login(server: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{server}/api/auth/device/code"))
        .json(&json!({
            "client_id": CLIENT_ID,
            "scope": "openid profile email",
        }))
        .send()
        .await
        .context("cannot reach the Readio cloud")?
        .error_for_status()
        .context("cloud refused device login")?
        .json::<DeviceCodeResponse>()
        .await
        .context("cloud returned an invalid device login response")?;

    let verification = response
        .verification_uri_complete
        .as_deref()
        .unwrap_or(&response.verification_uri);
    println!("Open: {verification}");
    println!("Code: {}", response.user_code);
    let _ = open_browser(verification);
    println!("Waiting for approval…");

    let interval = response.interval.unwrap_or(5).max(1);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(response.expires_in);
    loop {
        if tokio::time::Instant::now() >= deadline {
            bail!("device code expired; run `readio cloud login` again");
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
        let token_response = client
            .post(format!("{server}/api/auth/device/token"))
            .json(&json!({
                "grant_type": DEVICE_GRANT,
                "device_code": response.device_code,
                "client_id": CLIENT_ID,
            }))
            .send()
            .await
            .context("device authorization poll failed")?;
        if token_response.status().is_success() {
            let token = token_response.json::<DeviceTokenResponse>().await?;
            let state = CloudState {
                server: server.trim_end_matches('/').to_string(),
                access_token: token.access_token,
                device_id: new_device_id(),
                cursor: 0,
                replica: ReplicaSyncState::default(),
            };
            state.save()?;
            println!("Connected. Local reading is unchanged; cloud sync is now available.");
            return Ok(());
        }
        let error = token_response
            .json::<AuthError>()
            .await
            .unwrap_or(AuthError {
                error: None,
                code: None,
                message: None,
            });
        let code = error.error.or(error.code).unwrap_or_default();
        match code.as_str() {
            "authorization_pending" | "slow_down" => continue,
            "access_denied" => bail!("device authorization was denied"),
            "expired_token" => bail!("device code expired"),
            _ => bail!(
                "device authorization failed: {}",
                error.message.unwrap_or_else(|| code)
            ),
        }
    }
}

fn logout() -> Result<()> {
    let path = paths::cloud_file();
    match std::fs::remove_file(&path) {
        Ok(()) => println!("Cloud session removed. Local books and progress were kept."),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            println!("Already signed out. Local books and progress are available.")
        }
        Err(error) => {
            return Err(error).with_context(|| format!("cannot remove {}", path.display()));
        }
    }
    Ok(())
}

async fn status() -> Result<()> {
    let state = CloudState::load()?;
    let response = authorized(
        &state,
        reqwest::Client::new().get(format!("{}/api/auth/get-session", state.server)),
    )
    .send()
    .await?;
    if response.status().is_success() {
        println!("Connected to {} · cursor {}", state.server, state.cursor);
        Ok(())
    } else {
        bail!("cloud session expired; run `readio cloud login`")
    }
}

async fn sync() -> Result<()> {
    let mut diagnostics = DiagnosticLog::load_persisted().unwrap_or_default();
    let operation_id = diagnostics.begin_operation();
    let started_at = unix_time_milliseconds();
    let result = sync_operation(&mut diagnostics, &operation_id).await;
    match result {
        Ok(cursor) => {
            record_sync_fact(
                &mut diagnostics,
                &operation_id,
                "sync_completed",
                BTreeMap::from([
                    ("direction".to_string(), json!("sync")),
                    ("phase".to_string(), json!("complete")),
                    ("outcome".to_string(), json!("completed")),
                    ("cursor".to_string(), json!(cursor)),
                    (
                        "duration_ms".to_string(),
                        json!(unix_time_milliseconds().saturating_sub(started_at)),
                    ),
                ]),
            );
            println!("Cloud sync complete · cursor {cursor}");
            Ok(())
        }
        Err(error) => {
            record_sync_fact(
                &mut diagnostics,
                &operation_id,
                "sync_failed",
                BTreeMap::from([
                    ("direction".to_string(), json!("sync")),
                    ("phase".to_string(), json!("failed")),
                    ("outcome".to_string(), json!("failed")),
                    ("reason_code".to_string(), json!("sync_failed")),
                    (
                        "duration_ms".to_string(),
                        json!(unix_time_milliseconds().saturating_sub(started_at)),
                    ),
                ]),
            );
            Err(error)
        }
    }
}

async fn sync_operation(diagnostics: &mut DiagnosticLog, operation_id: &str) -> Result<u64> {
    let client = reqwest::Client::new();
    let mut state = CloudState::load()?;
    let library = Library::load();
    let mut store = Store::load();
    let (mut config, _) = Config::load();

    let pulled = pull_all(&client, &mut state).await?;
    record_sync_fact(
        diagnostics,
        operation_id,
        "pull_completed",
        BTreeMap::from([
            ("direction".to_string(), json!("pull")),
            ("phase".to_string(), json!("initial")),
            ("outcome".to_string(), json!("completed")),
            ("change_count".to_string(), json!(pulled)),
            ("cursor".to_string(), json!(state.cursor)),
        ]),
    );
    let local = local_sync_payloads(&library, &store, &config)
        .into_iter()
        .map(|entity| LocalReplicaEntity::new(entity.entity_kind, entity.entity_id, entity.payload))
        .collect::<Vec<_>>();
    state.replica.stage_local(&local)?;
    // The snapshot and outbox must reach durable storage before a request can
    // make the changes visible remotely. A crash then replays the same ids.
    state.save()?;
    apply_materialized_state(
        &state.replica.materialized_state()?,
        &mut store,
        &mut config,
    )?;
    store.save()?;
    config.save()?;

    let pushed = push_outbox(&client, &mut state).await?;
    record_sync_fact(
        diagnostics,
        operation_id,
        "push_completed",
        BTreeMap::from([
            ("direction".to_string(), json!("push")),
            ("phase".to_string(), json!("outbox")),
            ("outcome".to_string(), json!("completed")),
            ("change_count".to_string(), json!(pushed)),
            ("cursor".to_string(), json!(state.cursor)),
        ]),
    );
    // A push response is only a server high-water mark. Pull from the last
    // imported cursor before claiming that cursor in an R2 snapshot, otherwise
    // a concurrent device change could be skipped permanently.
    let pulled_after_push = pull_all(&client, &mut state).await?;
    record_sync_fact(
        diagnostics,
        operation_id,
        "post_push_pull_completed",
        BTreeMap::from([
            ("direction".to_string(), json!("pull")),
            ("phase".to_string(), json!("post_push")),
            ("outcome".to_string(), json!("completed")),
            ("change_count".to_string(), json!(pulled_after_push)),
            ("cursor".to_string(), json!(state.cursor)),
        ]),
    );
    apply_materialized_state(
        &state.replica.materialized_state()?,
        &mut store,
        &mut config,
    )?;
    store.save()?;
    config.save()?;
    upload_snapshot(&client, &state).await?;
    record_sync_fact(
        diagnostics,
        operation_id,
        "snapshot_uploaded",
        BTreeMap::from([
            ("direction".to_string(), json!("snapshot")),
            ("phase".to_string(), json!("snapshot")),
            ("outcome".to_string(), json!("completed")),
            ("cursor".to_string(), json!(state.cursor)),
        ]),
    );
    Ok(state.cursor)
}

async fn pull_all(client: &reqwest::Client, state: &mut CloudState) -> Result<usize> {
    let mut imported = 0;
    loop {
        let response = authorized(
            state,
            client.get(format!(
                "{}/v1/replica/pull?cursor={}&limit=100",
                state.server, state.cursor
            )),
        )
        .send()
        .await?
        .error_for_status()
        .context("cloud pull failed")?
        .json::<ReplicaPullResponse>()
        .await?;

        let changes = response
            .changes
            .into_iter()
            .map(|change| PendingReplicaChange {
                change_id: change.change_id,
                change_base64: change.change_base64,
            })
            .collect::<Vec<_>>();
        imported += changes.len();
        state.replica.import_remote(&changes)?;
        state.cursor = response.cursor;
        state.save()?;
        if !response.has_more {
            return Ok(imported);
        }
    }
}

fn apply_materialized_state(
    materialized: &Value,
    store: &mut Store,
    config: &mut Config,
) -> Result<()> {
    if let Some(positions) = materialized
        .get("reading_positions")
        .and_then(Value::as_object)
    {
        for (entity_id, payload) in positions {
            let remote: SyncedProgress = serde_json::from_value(payload.clone())?;
            let path = store
                .get(entity_id)
                .and_then(|progress| progress.path.clone());
            store.record(
                entity_id,
                Progress {
                    title: remote.title,
                    path,
                    chapter: remote.chapter,
                    para: remote.para,
                    chars_read: remote.chars_read,
                    sessions: remote.sessions,
                    updated: remote.updated,
                    locator: remote.locator,
                    speech: remote.speech,
                    marks: remote.marks,
                },
            );
        }
    }

    if let Some(payload) = materialized
        .get("settings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("preferences"))
    {
        let remote: SyncedSettings = serde_json::from_value(payload.clone())?;
        config.language = remote.language;
        config.reading = remote.reading;
        config.effort = remote.effort;
        config.images = remote.images;
        config.voice.engine = remote.voice.engine;
        config.voice.name = remote.voice.name;
        config.voice.language = remote.voice.language;
        config.voice.params = remote.voice.params;
        config.voice.prefetch = remote.voice.prefetch;
        config.voice.output = remote.voice.output;
        config.voice.books = remote.voice.books;
    }
    Ok(())
}

async fn push_outbox(client: &reqwest::Client, state: &mut CloudState) -> Result<usize> {
    let mut acknowledged = 0;
    loop {
        let changes = state.replica.outbox().iter().take(100).cloned().collect();
        let response = authorized(
            state,
            client.post(format!("{}/v1/replica/push", state.server)),
        )
        .json(&ReplicaPushBatch {
            device_id: state.device_id.clone(),
            ack_cursor: state.cursor,
            heads_base64: state.replica.heads_base64()?,
            changes,
        })
        .send()
        .await?
        .error_for_status()
        .context("cloud replica push failed")?
        .json::<ReplicaPushResponse>()
        .await?;
        let _server_high_water = response.cursor;
        let before = state.replica.outbox().len();
        acknowledged += response.accepted_change_ids.len();
        state.replica.acknowledge(&response.accepted_change_ids);
        state.save()?;

        if state.replica.outbox().is_empty() {
            return Ok(acknowledged);
        }
        if state.replica.outbox().len() == before {
            bail!("cloud did not acknowledge any replica changes");
        }
    }
}

fn record_sync_fact(
    diagnostics: &mut DiagnosticLog,
    operation_id: &str,
    name: &str,
    fields: BTreeMap<String, Value>,
) {
    diagnostics.record_fact(
        operation_id,
        "tui-replica-sync",
        DiagnosticCategory::Sync,
        name,
        fields,
    );
    // Diagnostics are observational: an unavailable support journal must not
    // turn a successful replica stage into a failed sync.
    let _ = diagnostics.persist_safe();
}

async fn upload_snapshot(client: &reqwest::Client, state: &CloudState) -> Result<()> {
    let snapshot = state.replica.snapshot_bytes()?;
    authorized(
        state,
        client.put(format!("{}/v1/replica/snapshot", state.server)),
    )
    .header("content-type", "application/octet-stream")
    .header("x-readio-device-id", &state.device_id)
    .header("x-readio-replica-cursor", state.cursor)
    .body(snapshot)
    .send()
    .await?
    .error_for_status()
    .context("cloud replica snapshot upload failed")?;
    Ok(())
}

fn authorized(state: &CloudState, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    request.bearer_auth(&state.access_token)
}

fn new_device_id() -> String {
    let material = format!(
        "{}:{}:{}",
        paths::home().display(),
        std::process::id(),
        now_millis()
    );
    let digest = format!("{:x}", Sha256::digest(material.as_bytes()));
    format!("tui-{}", &digest[..24])
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(url).status();
    #[cfg(target_os = "linux")]
    let status = Command::new("xdg-open").arg(url).status();
    #[cfg(target_os = "windows")]
    let status = Command::new("cmd").args(["/C", "start", "", url]).status();
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let status: std::io::Result<std::process::ExitStatus> = Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "browser launch is unsupported",
    ));

    match status {
        Ok(exit) if exit.success() => Ok(()),
        Ok(_) => Err(anyhow!("browser command failed")),
        Err(error) => Err(error.into()),
    }
}
