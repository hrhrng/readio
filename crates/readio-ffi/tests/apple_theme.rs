#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_theme_projects_the_committed_readio_palette_in_both_appearances() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let apple = repo_root.join("apps/ios/Readio/Readio");
    let fixture_dir = env::temp_dir().join(format!(
        "readio-apple-theme-{}",
        std::process::id()
    ));
    fs::create_dir_all(&fixture_dir).expect("theme fixture directory");
    let fixture = fixture_dir.join("AppleThemeSmoke.swift");
    fs::write(&fixture, SWIFT_FIXTURE).expect("theme fixture source");
    let executable = fixture_dir.join("apple-theme-smoke");
    let compile = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg(apple.join("Views/Common/ReadioAppleTheme.swift"))
        .arg(&fixture)
        .arg("-framework")
        .arg("SwiftUI")
        .arg("-framework")
        .arg("AppKit")
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify the Apple theme");
    assert!(
        compile.status.success(),
        "the Apple theme must compile as a reusable product contract:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&executable)
        .output()
        .expect("the Apple theme fixture can run");
    let _ = fs::remove_dir_all(&fixture_dir);
    assert!(
        run.status.success(),
        "the Apple theme must preserve the committed light/dark colors:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

const SWIFT_FIXTURE: &str = r##"
import SwiftUI
import AppKit

@main
enum AppleThemeSmoke {
    static func main() {
        let light = ReadioAppleTheme.spec(for: .light)
        let dark = ReadioAppleTheme.spec(for: .dark)
        let shelfAction = NSColor(
            ReadioAppleTheme.palette(for: .light).shelfAction
        ).usingColorSpace(.sRGB)
        guard light.accentHex == "#397548",
              light.highlightHex == "#BFD9C4",
              light.canvasHex == "#FCFCF8",
              light.sidebarHex == "#F7F7F2",
              light.sidebarInkHex == "#292929",
              light.sidebarSelectedHex == "#E0EBE1",
              light.selectedHex == "#E0EBE1",
              light.inkHex == "#292929",
              dark.accentHex == "#8AD6A9",
              dark.canvasHex == "#181C19",
              dark.sidebarHex == "#0F1912",
              dark.sidebarInkHex == "#F4F8F4",
              dark.sidebarSelectedHex == "#244D34",
              dark.raisedHex == "#252925",
              dark.inkHex == "#F4F7F4",
              ReadioAppleLayout.sidebarIdealWidth == 256,
              ReadioAppleLayout.libraryContentMaxWidth == 1180,
              ReadioAppleLayout.bookCoverWidth == 200,
              ReadioAppleLayout.bookCoverHeight == 300,
              ReadioAppleTheme.normalizedAccentID("blue") == "clover",
              ReadioAppleTheme.normalizedAccentID("#4F46E5") == "clover",
              let shelfAction,
              abs(shelfAction.redComponent - 57.0 / 255.0) < 0.001,
              abs(shelfAction.greenComponent - 117.0 / 255.0) < 0.001,
              abs(shelfAction.blueComponent - 72.0 / 255.0) < 0.001 else {
            fatalError("Apple theme drifted from the Readio design contract")
        }
    }
}
"##;
