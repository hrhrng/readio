import Foundation
import SwiftUI

// MARK: - HomeViewModel
/// View model for the Home / Dashboard screen.
///
/// The home screen shows two curated sections:
///
/// 1. **Recent Items** — the 10 most recently updated library items, sorted by
///    `created_at` descending. This gives the user quick access to newly imported
///    content regardless of reading progress.
///
/// 2. **In Progress** — items with reading progress between 1% and 99% (exclusive),
///    sorted by most recently updated. These are the user's "currently reading" items
///    that they're most likely to want to resume.
///
/// Both sections use `LibraryItemSummary` (the lightweight model without content)
/// to minimize data transfer. The full `LibraryItem` is only fetched when the user
/// taps through to the reader/player screen.
///
/// ## Data Flow
///
/// ```
/// HomeView  →  HomeViewModel.loadDashboard()
///                  ├─ LibraryService.fetchItems(sort_by: created_at, limit: 10)  → recentItems
///                  └─ LibraryService.fetchItems(progress_min: 1, progress_max: 99) → inProgressItems
/// ```
///
/// Both requests are fired concurrently via `async let` for faster initial load.
@MainActor
@Observable
final class HomeViewModel {

    // MARK: - Published State

    /// The 10 most recently created/imported library items.
    /// Displayed in a horizontal scroll or grid at the top of the home screen.
    var recentItems: [LibraryItemSummary] = []

    /// Items currently being read (1% <= progress <= 99%).
    /// Displayed as a "Continue Reading" section below the recent items.
    var inProgressItems: [LibraryItemSummary] = []

    /// Whether the initial data load is in progress.
    /// The view shows a skeleton/shimmer placeholder while this is `true`.
    var isLoading = false

    /// Non-nil when a network or decoding error occurred during loading.
    /// Cleared on the next successful `loadDashboard()` call.
    var error: String? = nil

    // MARK: - Dependencies

    /// Service layer for fetching library items from the backend API.
    private let libraryService = LibraryService()

    // MARK: - Public API

    /// Fetch both dashboard sections concurrently.
    ///
    /// Called on view appear and on pull-to-refresh. Both API calls run in
    /// parallel via `async let` — if one fails, the other's results are still
    /// displayed (partial success is better than a blank screen).
    func loadDashboard() async {
        isLoading = true
        error = nil

        do {
            // Fire both requests concurrently for faster loading.
            // Recent: sorted by creation date, take the first 10.
            // In-progress: only items with 1-99% progress.
            async let recentResponse = libraryService.fetchItems(
                page: 1,
                pageSize: 10,
                sortBy: "created_at",
                sortOrder: "desc"
            )

            async let progressResponse = libraryService.fetchItems(
                page: 1,
                pageSize: 20,
                sortBy: "created_at",
                sortOrder: "desc",
                progressMin: 1,
                progressMax: 99
            )

            // Await both results — if either throws, the `catch` block handles it
            let (recent, progress) = try await (recentResponse, progressResponse)

            recentItems = recent.items
            inProgressItems = progress.items
        } catch {
            self.error = "Failed to load dashboard: \(error.localizedDescription)"
        }

        isLoading = false
    }
}
