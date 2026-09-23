#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_playback_executor_runs_the_real_rust_generation_protocol() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let sdk = repo_root.join("apps/ios/Readio/Readio/Core/ReadioSDK.swift");
    let executor = repo_root.join("apps/ios/Readio/Readio/Core/ReadioPlaybackExecutor.swift");
    let diagnostics = repo_root.join("apps/ios/Readio/Readio/Core/AppleDiagnostics.swift");
    let include_dir = manifest_dir.join("include");
    let deps_dir = env::current_exe()
        .expect("test executable")
        .parent()
        .expect("deps directory")
        .to_owned();
    let static_library = deps_dir.join("libreadio_ffi.a");
    let fixture_dir = env::temp_dir().join(format!(
        "readio-apple-playback-executor-{}",
        std::process::id()
    ));
    fs::create_dir_all(&fixture_dir).expect("fixture directory");
    let fixture = fixture_dir.join("ApplePlaybackSmoke.swift");
    let executable = fixture_dir.join("apple-playback-smoke");
    fs::write(
        &fixture,
        r#"
import Foundation

@MainActor
final class RecordingPlaybackPlatform: ReadioPlaybackPlatform {
    var audioPositionMilliseconds: UInt64 = 0
    var events: [String] = []

    func fetchAudio(sentence: Int, generation: UInt64, priority: String) {
        events.append("fetch:\(sentence):\(generation):\(priority)")
    }

    func playAudio(sentence: Int, generation: UInt64, offsetMilliseconds: UInt64) {
        events.append("play:\(sentence):\(generation):\(offsetMilliseconds)")
    }

    func scheduleTick(generation: UInt64, afterMilliseconds: UInt64) {
        events.append("tick:\(generation):\(afterMilliseconds)")
    }

    func pauseAudio() {
        events.append("pause")
    }

    func persistProgress(sentence: Int, positionMilliseconds: UInt64) {
        events.append("persist:\(sentence):\(positionMilliseconds)")
    }

    func prefetchAudio(sentences: [Int]) {
        events.append("prefetch:\(sentences.map(String.init).joined(separator: ","))")
    }

    func playbackFailed(sentence: Int, message: String) {
        events.append("failed:\(sentence):\(message)")
    }

    func playbackCompleted() {
        events.append("completed")
    }

    func playbackStateChanged(_ state: ReadioPlaybackState) {}
}

@main
@MainActor
struct ApplePlaybackSmoke {
    static func main() async throws {
        let platform = RecordingPlaybackPlatform()
        let diagnostics = AppleDiagnosticLog(
            now: { 125 },
            makeOperationID: { "op-apple-playback" }
        )
        let executor = try ReadioPlaybackExecutor(
            sentenceCount: 2,
            platform: platform,
            diagnostics: diagnostics
        )

        try executor.play()
        try executor.audioReady(sentence: 0, generation: 1, durationMilliseconds: 2_000)
        try executor.setSleepDeadline(unixMilliseconds: 50_000)
        guard executor.state.sleepDeadlineUnixMilliseconds == 50_000 else {
            throw SmokeError.unexpectedSleepDeadline
        }
        try executor.clearSleepDeadline()
        guard executor.state.sleepDeadlineUnixMilliseconds == nil else {
            throw SmokeError.unexpectedSleepDeadline
        }
        platform.audioPositionMilliseconds = 701
        try executor.pause()
        try executor.play()

        let beforeStaleEnd = platform.events
        try executor.audioEnded(sentence: 0, generation: 1)
        guard platform.events == beforeStaleEnd else {
            throw SmokeError.staleEffect
        }
        guard executor.state.status == .playing,
              executor.state.currentSentence == 0,
              executor.state.currentPositionMilliseconds == 701,
              executor.state.generation == 3 else {
            throw SmokeError.unexpectedState
        }
        guard platform.events == [
            "fetch:0:1:user",
            "prefetch:1",
            "play:0:1:0",
            "tick:1:250",
            "pause",
            "persist:0:701",
            "play:0:3:701",
            "tick:3:250",
        ] else {
            throw SmokeError.unexpectedEffects(platform.events)
        }

        try executor.audioFailed(
            sentence: 0,
            generation: 3,
            message: "provider unavailable"
        )
        let diagnosticEvents = await diagnostics.snapshot()
        guard diagnosticEvents.allSatisfy({
                  $0.operationID == "op-apple-playback"
              }),
              diagnosticEvents.contains(where: { event in
                  event.category == .playback
                      && event.name == "clock_sample"
                      && event.fields["generation"] == .number(3)
                      && event.fields["sentence_index"] == .number(0)
                      && event.fields["media_position_ms"] == .number(701)
                      && event.fields["playing"] == .boolean(false)
              }),
              diagnosticEvents.contains(where: { event in
                  event.category == .fallback
                      && event.name == "provider_fallback_stopped"
                      && event.fields["from_provider"] == .string("apple_native")
                      && event.fields["to_provider"] == .string("none")
                      && event.fields["outcome"] == .string("stopped")
              }) else {
            throw SmokeError.missingDiagnostics
        }
    }
}

enum SmokeError: Error {
    case staleEffect
    case unexpectedState
    case unexpectedSleepDeadline
    case unexpectedEffects([String])
    case missingDiagnostics
}
"#,
    )
    .expect("write fixture");

    let compile = Command::new("swiftc")
        .arg("-I")
        .arg(&include_dir)
        .arg(&sdk)
        .arg(&diagnostics)
        .arg(&executor)
        .arg(&fixture)
        .arg(&static_library)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required");
    assert!(
        compile.status.success(),
        "Apple playback executor must compile against the real Rust SDK:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("run Swift playback fixture");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "Apple playback executor must run Rust effects:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}
