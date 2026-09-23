#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_reader_surface_keeps_media_fallbacks_and_desktop_chrome_compact() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let policy = repo_root.join("apps/ios/Readio/Readio/Views/Common/AppleRootLayoutPolicy.swift");
    let fixture_dir = env::temp_dir().join(format!(
        "readio-apple-reader-surface-policy-{}",
        std::process::id()
    ));
    fs::create_dir_all(&fixture_dir).expect("reader surface fixture directory");
    let fixture = fixture_dir.join("ReaderSurfaceSmoke.swift");
    let executable = fixture_dir.join("reader-surface-smoke");
    fs::write(
        &fixture,
        r#"
import SwiftUI

@main
enum ReaderSurfaceSmoke {
    static func main() {
        guard AppleReaderSurfacePolicy.imagePresentation(for: .loading) == .placeholder,
              AppleReaderSurfacePolicy.imagePresentation(for: .loaded) == .content,
              AppleReaderSurfacePolicy.imagePresentation(for: .failed) == .fallback,
              AppleReaderSurfacePolicy.imagePresentation(for: .unresolved) == .fallback else {
            fatalError("failed reader images must keep a compact semantic fallback")
        }

        guard AppleReaderSurfacePolicy.remoteImageURL("https://example.com/cover.png") != nil,
              AppleReaderSurfacePolicy.remoteImageURL("images/terminal.png") == nil,
              AppleReaderSurfacePolicy.remoteImageURL(nil) == nil else {
            fatalError("unresolvable Markdown-relative media must not reach AsyncImage")
        }

        guard AppleReaderSurfacePolicy.hidesNativeWindowToolbar,
              AppleReaderSurfacePolicy.toolbarRevealHeight == 44,
              AppleReaderSurfacePolicy.toolbarVisibility(pointerInside: false) == .hidden,
              AppleReaderSurfacePolicy.toolbarVisibility(pointerInside: true) == .visible,
              AppleReaderSurfacePolicy.playerDockVisibility(pointerInside: false) == .hidden,
              AppleReaderSurfacePolicy.playerDockVisibility(pointerInside: true) == .visible,
              AppleReaderSurfacePolicy.hoverTargets(
                  locationY: 360,
                  viewportHeight: 720
              ) == AppleReaderHoverTargets(toolbar: false, playerDock: false),
              AppleReaderSurfacePolicy.hoverTargets(
                  locationY: 20,
                  viewportHeight: 720
              ) == AppleReaderHoverTargets(toolbar: true, playerDock: false),
              AppleReaderSurfacePolicy.hoverTargets(
                  locationY: 700,
                  viewportHeight: 720
              ) == AppleReaderHoverTargets(toolbar: false, playerDock: true),
              AppleReaderSurfacePolicy.hoverTargets(
                  locationY: 630,
                  viewportHeight: 720,
                  playerDockVisible: false
              ) == AppleReaderHoverTargets(toolbar: false, playerDock: false),
              AppleReaderSurfacePolicy.hoverTargets(
                  locationY: 630,
                  viewportHeight: 720,
                  playerDockVisible: true
              ) == AppleReaderHoverTargets(toolbar: false, playerDock: true),
              AppleReaderSurfacePolicy.homeShelfHeaderGap == 12,
              !AppleReaderSurfacePolicy.showsHomeScrollIndicator,
              AppleReaderSurfacePolicy.homeShelfWidth(itemCount: 1, cardWidth: 200) == 320,
              AppleReaderSurfacePolicy.homeShelfWidth(itemCount: 3, cardWidth: 200) == 660,
              AppleReaderSurfacePolicy.homeShelfWidth(itemCount: 20, cardWidth: 200) == 860 else {
            fatalError("desktop chrome and home shelf geometry must stay bounded")
        }
    }
}
"#,
    )
    .expect("reader surface fixture source");

    let compile = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg(&policy)
        .arg(&fixture)
        .arg("-framework")
        .arg("SwiftUI")
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify the Apple reader surface policy");
    assert!(
        compile.status.success(),
        "Apple reader surface policy must compile:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&executable)
        .output()
        .expect("reader surface fixture can run");
    let _ = fs::remove_dir_all(fixture_dir);
    assert!(
        run.status.success(),
        "Apple reader surface policy must preserve compact failure states:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}
