#![cfg(target_vendor = "apple")]

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn shared_apple_surface_links_as_a_native_macos_application() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let apple = repo_root.join("apps/ios/Readio/Readio");
    let mut production_sources = Vec::new();
    collect_swift_sources(&apple, &mut production_sources);
    production_sources.sort();
    let fixture_dir = env::temp_dir().join(format!(
        "readio-native-macos-surface-{}",
        std::process::id()
    ));
    fs::create_dir_all(&fixture_dir).expect("macOS linkage fixture directory");
    let executable = fixture_dir.join("Readio");
    let deps_dir = env::current_exe()
        .expect("test executable path")
        .parent()
        .expect("Cargo deps directory")
        .to_owned();
    let static_library = deps_dir.join("libreadio_ffi.a");
    let sources: Vec<_> = production_sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let contents = fs::read_to_string(source).expect("Swift production source");
            // Command Line Tools do not ship Xcode's PreviewsMacros plugin.
            // Preview declarations are design-time only; typecheck every
            // production declaration that precedes the first preview.
            let production = contents
                .split_once("\n#Preview")
                .map_or(contents.as_str(), |(value, _)| value);
            let destination = fixture_dir.join(format!(
                "{index:03}-{}",
                source
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("Source.swift")
            ));
            fs::write(&destination, production).expect("sanitized Swift source");
            destination
        })
        .collect();

    let mut command = Command::new("swiftc");
    command
        .arg("-parse-as-library")
        .arg("-module-name")
        .arg("Readio")
        .arg("-I")
        .arg(manifest_dir.join("include"));
    command.args(sources);
    command
        .arg(&static_library)
        .arg("-framework")
        .arg("AVFoundation")
        .arg("-framework")
        .arg("MediaPlayer")
        .arg("-framework")
        .arg("SwiftUI")
        .arg("-framework")
        .arg("ImageIO")
        .arg("-o")
        .arg(&executable);

    let result = command
        .output()
        .expect("swiftc is required to verify the native macOS surface");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        result.status.success(),
        "the shared Apple product must link against the native macOS SDK and Rust core:\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn apple_content_model_keeps_optional_metadata_optional_at_call_sites() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let apple = repo_root.join("apps/ios/Readio/Readio");
    let fixture_dir = env::temp_dir().join(format!(
        "readio-apple-heading-default-{}",
        std::process::id()
    ));
    fs::create_dir_all(&fixture_dir).expect("heading fixture directory");
    let fixture = fixture_dir.join("HeadingDefault.swift");
    fs::write(
        &fixture,
        r#"
import Foundation

@main
enum HeadingDefault {
    static func main() {
        let block = ContentBlock.heading(level: 1, sentences: [])
        _ = ContentBlock.paragraph(sentences: [])
        _ = ContentBlock.image(src: "cover.png", alt: "Cover")
        _ = ContentBlock.list(ordered: false, items: [])
        _ = ContentBlock.blockquote(sentences: [])
        _ = ContentBlock.code(text: "let portable = true")
        guard case let .heading(level, _, anchorID) = block,
              level == 1,
              anchorID == nil else {
            fatalError("the omitted optional anchor must resolve to nil")
        }
    }
}
"#,
    )
    .expect("heading fixture source");
    let executable = fixture_dir.join("heading-default");
    let result = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg(apple.join("Models/ContentBlock.swift"))
        .arg(&fixture)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify the Apple content model");
    if !result.status.success() {
        let _ = fs::remove_dir_all(&fixture_dir);
        panic!(
            "Apple content call sites must be able to omit optional metadata:\n{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let run = Command::new(&executable)
        .output()
        .expect("heading fixture can run");
    let _ = fs::remove_dir_all(&fixture_dir);
    assert!(run.status.success(), "heading default must be nil");
}

#[test]
fn command_line_packager_builds_a_signed_native_macos_bundle_without_xcode() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let deps_dir = env::current_exe()
        .expect("test executable path")
        .parent()
        .expect("Cargo deps directory")
        .to_owned();
    let fixture_dir = env::temp_dir().join(format!(
        "readio-macos-bundle-{}",
        std::process::id()
    ));
    fs::create_dir_all(&fixture_dir).expect("bundle fixture directory");
    let bundle = fixture_dir.join("Readio.app");
    let result = Command::new("/bin/bash")
        .arg(repo_root.join("apps/ios/scripts/build-macos-app-with-swiftc.sh"))
        .arg("--output")
        .arg(&bundle)
        .env("READIO_FFI_LIBRARY", deps_dir.join("libreadio_ffi.a"))
        .output()
        .expect("the command-line macOS packager can run");
    assert!(
        result.status.success(),
        "the command-line packager must build a complete app bundle:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(bundle.join("Contents/MacOS/Readio").is_file());
    assert!(bundle.join("Contents/Info.plist").is_file());
    assert!(
        bundle
            .join("Contents/Resources/en.lproj/Localizable.strings")
            .is_file(),
        "the signed bundle must contain the English UI catalog"
    );
    assert!(
        bundle
            .join("Contents/Resources/zh-Hans.lproj/Localizable.strings")
            .is_file(),
        "the signed bundle must contain the Simplified Chinese UI catalog"
    );
    let verify = Command::new("codesign")
        .arg("--verify")
        .arg("--deep")
        .arg("--strict")
        .arg(&bundle)
        .output()
        .expect("codesign is available on macOS");
    let _ = fs::remove_dir_all(&fixture_dir);
    assert!(
        verify.status.success(),
        "the app bundle must carry a valid ad-hoc signature:\n{}",
        String::from_utf8_lossy(&verify.stderr)
    );
}

fn collect_swift_sources(directory: &Path, output: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("Apple source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            collect_swift_sources(&path, output);
        } else if path.extension().and_then(|value| value.to_str()) == Some("swift") {
            output.push(path);
        }
    }
}
