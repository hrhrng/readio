#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_text_executor_runs_the_real_rust_deadline_protocol() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let sdk = repo_root.join("apps/ios/Readio/Readio/Core/ReadioSDK.swift");
    let executor = repo_root.join("apps/ios/Readio/Readio/Core/ReadioTextAdvanceExecutor.swift");
    let include_dir = manifest_dir.join("include");
    let deps_dir = env::current_exe()
        .expect("test executable")
        .parent()
        .expect("deps directory")
        .to_owned();
    let static_library = deps_dir.join("libreadio_ffi.a");
    let fixture_dir = env::temp_dir().join(format!(
        "readio-apple-text-advance-executor-{}",
        std::process::id()
    ));
    fs::create_dir_all(&fixture_dir).expect("fixture directory");
    let fixture = fixture_dir.join("AppleTextAdvanceSmoke.swift");
    let executable = fixture_dir.join("apple-text-advance-smoke");
    fs::write(
        &fixture,
        r#"
import Foundation

@MainActor
final class RecordingTextPlatform: ReadioTextAdvancePlatform {
    var monotonicMilliseconds: UInt64 = 10_000
    var events: [String] = []

    func scheduleAdvance(generation: UInt64, afterMilliseconds: UInt64) {
        events.append("schedule:\(generation):\(afterMilliseconds)")
    }

    func cancelScheduledAdvance() {
        events.append("cancel")
    }

    func persistTextProgress(sentence: Int) {
        events.append("persist:\(sentence)")
    }

    func textAdvanceCompleted() {
        events.append("completed")
    }

    func textAdvanceStateChanged(_ state: ReadioTextAdvanceState) {}
}

@main
@MainActor
struct AppleTextAdvanceSmoke {
    static func main() throws {
        let platform = RecordingTextPlatform()
        let executor = try ReadioTextAdvanceExecutor(
            characterCounts: [80, 40],
            charactersPerSecond: 40,
            platform: platform
        )

        try executor.start()
        platform.monotonicMilliseconds = 10_500
        try executor.pause()
        platform.monotonicMilliseconds = 20_000
        try executor.start()
        platform.monotonicMilliseconds = 21_500
        try executor.advanceDue(generation: 3)

        guard executor.state.status == .running,
              executor.state.currentSentence == 1,
              executor.state.generation == 4 else {
            throw SmokeError.unexpectedState
        }
        guard platform.events == [
            "schedule:1:2000",
            "cancel",
            "schedule:3:1500",
            "persist:1",
            "schedule:4:1000",
        ] else {
            throw SmokeError.unexpectedEffects(platform.events)
        }
    }
}

enum SmokeError: Error {
    case unexpectedState
    case unexpectedEffects([String])
}
"#,
    )
    .expect("write fixture");

    let compile = Command::new("swiftc")
        .arg("-I")
        .arg(&include_dir)
        .arg(&sdk)
        .arg(&executor)
        .arg(&fixture)
        .arg(&static_library)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required");
    assert!(
        compile.status.success(),
        "Apple text executor must compile against the real Rust SDK:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("run Swift text fixture");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "Apple text executor must run Rust deadline effects:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}
