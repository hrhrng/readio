#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_sdk_uses_installed_native_voices_and_renders_reusable_audio() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("the FFI crate lives under the repository crates directory");
    let apple_core = repo_root.join("apps/ios/Readio/Readio/Core");
    let provider = repo_root.join("apps/ios/Readio/Readio/Core/AppleNativeTTS.swift");
    let include_dir = manifest_dir.join("include");
    let deps_dir = env::current_exe()
        .expect("the test executable has a path")
        .parent()
        .expect("the test executable lives in Cargo's deps directory")
        .to_owned();
    let static_library = deps_dir.join("libreadio_ffi.a");
    let fixture_dir =
        env::temp_dir().join(format!("readio-apple-native-tts-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("the Swift fixture directory can be created");
    let fixture = fixture_dir.join("NativeTTSSmoke.swift");
    let executable = fixture_dir.join("native-tts-smoke");
    fs::write(&fixture, SWIFT_FIXTURE).expect("the Swift fixture can be written");

    let compile = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg("-I")
        .arg(&include_dir)
        .arg(apple_core.join("ReadioSDK.swift"))
        .arg(apple_core.join("AppleDiagnostics.swift"))
        .arg(&provider)
        .arg(&fixture)
        .arg(&static_library)
        .arg("-framework")
        .arg("AVFoundation")
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify native Apple TTS");
    assert!(
        compile.status.success(),
        "the native TTS provider must compile as part of the Apple SDK:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("the native TTS fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "the Apple SDK must enumerate and render a system voice:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

const SWIFT_FIXTURE: &str = r#"
import AVFoundation
import Foundation

@main
enum NativeTTSSmoke {
    @MainActor
    static func main() async throws {
        AppleDiagnosticLog.shared.clear()
        let provider = AppleNativeTTSProvider()
        let catalog = provider.voiceCatalog()
        guard let voice = catalog.voices.first,
              !voice.id.isEmpty,
              !voice.language.isEmpty,
              !catalog.defaultVoiceID.isEmpty else {
            throw FixtureError.missingSystemVoice
        }

        let request = ReadioNativeSpeechRequest(
            text: "Hello from Readio.",
            voiceID: voice.id,
            language: voice.language,
            rateMultiplier: 1,
            pitchMultiplier: 1,
            volume: 1
        )
        let utterance = provider.makeUtterance(request)
        guard utterance.voice?.identifier == voice.id,
              utterance.rate == AVSpeechUtteranceDefaultSpeechRate,
              utterance.pitchMultiplier == 1,
              utterance.volume == 1 else {
            throw FixtureError.invalidUtterance
        }

        let audio = try await withThrowingTaskGroup(
            of: ReadioNativeSpeechAudio.self
        ) { group in
            group.addTask { try await provider.render(request) }
            group.addTask {
                try await Task.sleep(for: .seconds(10))
                throw FixtureError.renderTimedOut
            }
            let first = try await group.next()!
            group.cancelAll()
            return first
        }
        guard audio.data.count > 64,
              audio.durationMilliseconds > 0,
              audio.sampleRate > 0 else {
            throw FixtureError.invalidAudio
        }
        let events = AppleDiagnosticLog.shared.snapshot()
        guard events.map(\.name) == ["job_requested", "job_completed"],
              let operationID = events.first?.operationID,
              events.allSatisfy({ event in
                  event.operationID == operationID
                      && event.category == .tts
                      && event.fields["provider"] == .string("apple_native")
              }),
              events.last?.fields["audio_duration_ms"]
                  == .number(audio.durationMilliseconds) else {
            throw FixtureError.missingDiagnostics
        }
        let encoded = String(
            decoding: try JSONEncoder().encode(events),
            as: UTF8.self
        )
        guard !encoded.contains("Hello from Readio.") else {
            throw FixtureError.privateDiagnostics
        }
    }
}

enum FixtureError: Error {
    case missingSystemVoice
    case invalidUtterance
    case invalidAudio
    case renderTimedOut
    case missingDiagnostics
    case privateDiagnostics
}
"#;
