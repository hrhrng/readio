#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_empty_states_offer_the_sample_without_polluting_filtered_results() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let policy =
        repo_root.join("apps/ios/Readio/Readio/Views/Common/AppleBuiltinSampleEntry.swift");
    let fixture_dir =
        env::temp_dir().join(format!("readio-apple-sample-entry-{}", std::process::id()));
    fs::create_dir_all(&fixture_dir).expect("fixture directory");
    let fixture = fixture_dir.join("SampleEntrySmoke.swift");
    let executable = fixture_dir.join("sample-entry-smoke");
    fs::write(
        &fixture,
        r#"
@main
enum SampleEntrySmoke {
    static func main() {
        guard AppleBuiltinSampleEntry.destinationItemID(
            libraryIsEmpty: true,
            searchOrFilterIsActive: false
        ) == "readio:sample:v1" else {
            fatalError("an unfiltered empty library must offer the virtual sample")
        }
        guard AppleBuiltinSampleEntry.destinationItemID(
            libraryIsEmpty: false,
            searchOrFilterIsActive: false
        ) == nil else {
            fatalError("the sample must not enter a populated library")
        }
        guard AppleBuiltinSampleEntry.destinationItemID(
            libraryIsEmpty: true,
            searchOrFilterIsActive: true
        ) == nil else {
            fatalError("the sample must not masquerade as a filtered result")
        }
    }
}
"#,
    )
    .expect("fixture source");

    let compile = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg(&policy)
        .arg(&fixture)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required");
    assert!(
        compile.status.success(),
        "Apple sample entry policy must compile:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("sample entry fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "Apple sample entry policy failed:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}
