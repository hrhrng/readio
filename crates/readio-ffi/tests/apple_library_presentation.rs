#![cfg(target_vendor = "apple")]

use std::{env, fs, path::PathBuf, process::Command};

#[test]
fn apple_library_loads_real_in_progress_summary_for_the_feature_strip() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo provides the manifest path"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("repository root");
    let apple = repo_root.join("apps/ios/Readio/Readio");
    let fixture_dir = env::temp_dir().join(format!(
        "readio-apple-library-presentation-{}",
        std::process::id()
    ));
    fs::create_dir_all(&fixture_dir).expect("library presentation fixture directory");
    let fixture = fixture_dir.join("LibraryPresentationSmoke.swift");
    fs::write(&fixture, SWIFT_FIXTURE).expect("library presentation fixture source");
    let executable = fixture_dir.join("apple-library-presentation-smoke");
    let compile = Command::new("swiftc")
        .arg("-parse-as-library")
        .arg(apple.join("ViewModels/LibraryViewModel.swift"))
        .arg(&fixture)
        .arg("-framework")
        .arg("SwiftUI")
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("swiftc is required to verify the Apple library presentation");
    assert!(
        compile.status.success(),
        "the Apple library presentation must compile against its real view model:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&executable)
        .output()
        .expect("the Apple library presentation fixture can run");
    let _ = fs::remove_dir_all(&fixture_dir);
    assert!(
        run.status.success(),
        "the Apple library must expose honest in-progress totals and resume item:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

const SWIFT_FIXTURE: &str = r##"
import Foundation

enum LibraryItemType: String, Sendable { case epub, pdf }
enum LibraryItemCategory: String, Sendable { case imported }
enum ReadingStatus: String, Sendable { case new_, reading, finished }

struct LibraryItemSummary: Identifiable, Sendable {
    let id: String
    let title: String
    let progress: Int

    var readingStatus: ReadingStatus {
        if progress <= 0 { return .new_ }
        if progress >= 100 { return .finished }
        return .reading
    }
}

struct LibraryItemsResponse: Sendable {
    let items: [LibraryItemSummary]
    let total: Int
    let page: Int
    let pageSize: Int
    let totalPages: Int
}

struct LibraryService: Sendable {
    func fetchItems(
        category: String? = nil,
        search: String? = nil,
        folderId: String? = nil,
        type: String? = nil,
        page: Int? = nil,
        pageSize: Int? = nil,
        sortBy: String? = nil,
        sortOrder: String? = nil,
        progressMin: Int? = nil,
        progressMax: Int? = nil
    ) async throws -> LibraryItemsResponse {
        if progressMin == 1 && progressMax == 99 {
            return LibraryItemsResponse(
                items: [LibraryItemSummary(id: "resume", title: "Ways of Seeing", progress: 42)],
                total: 3,
                page: 1,
                pageSize: 1,
                totalPages: 3
            )
        }
        return LibraryItemsResponse(
            items: [
                LibraryItemSummary(id: "new", title: "New Book", progress: 0),
                LibraryItemSummary(id: "resume", title: "Ways of Seeing", progress: 42),
            ],
            total: 12,
            page: 1,
            pageSize: 20,
            totalPages: 1
        )
    }

    func deleteItem(id: String) async throws {}
}

@main
enum LibraryPresentationSmoke {
    @MainActor
    static func main() async {
        let viewModel = LibraryViewModel()
        await viewModel.loadItems()
        guard viewModel.totalItems == 12,
              viewModel.inProgressTotal == 3,
              viewModel.continueReadingItem?.id == "resume",
              viewModel.continueReadingItem?.progress == 42 else {
            fatalError("Library presentation is not driven by the real query response")
        }
    }
}
"##;
