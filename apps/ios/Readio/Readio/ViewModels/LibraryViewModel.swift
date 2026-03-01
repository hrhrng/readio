import Foundation
import SwiftUI

// MARK: - LibraryViewModel
/// View model for the Library list/grid screen with pagination, filtering, and search.
///
/// The library screen is the primary content browser. It displays a paginated list
/// of all library items with support for:
///
/// - **Category filtering:** Show all items, or filter by "imported" / "podcasts".
/// - **Type filtering:** Show all types, or filter by "web", "txt", "pdf", "epub".
/// - **Text search:** Full-text search via the backend's search parameter.
/// - **Sorting:** By creation date, title, or progress, in ascending or descending order.
/// - **Pagination:** Offset-based pagination with infinite scroll ("load more" at bottom).
/// - **Deletion:** Swipe-to-delete with backend synchronization.
/// - **Pull-to-refresh:** Resets to page 1 and reloads with current filters.
///
/// ## Pagination Model
///
/// The backend uses 1-based offset pagination:
/// - `page`: current page number (starts at 1)
/// - `page_size`: items per page (default 20)
/// - `total_pages`: ceil(total / page_size), returned in every response
///
/// The view model tracks `currentPage` and `totalPages` to enable/disable
/// the "load more" button and to append (not replace) items on subsequent pages.
///
/// ## Data Flow
///
/// ```
/// LibraryView
///   ├─ .onAppear → loadItems()          // initial load, page 1
///   ├─ .onReachBottom → loadNextPage()   // append next page
///   ├─ .onSearch → search(query)         // reset + filter by text
///   ├─ .onFilter → loadItems()           // reset + apply filter
///   ├─ .onRefresh → refresh()            // pull-to-refresh, page 1
///   └─ .onDelete → deleteItem(id:)       // remove from backend + local list
/// ```
@MainActor
@Observable
final class LibraryViewModel {

    // MARK: - Published State

    /// The currently loaded library items (accumulates across pages for infinite scroll).
    var items: [LibraryItemSummary] = []

    /// Total number of items matching the current filters (across all pages).
    var totalItems = 0

    /// Current page number (1-based). Incremented by `loadNextPage()`.
    var currentPage = 1

    /// Total pages available for the current filter set.
    var totalPages = 1

    /// Whether a network request is in progress (initial load or next page).
    var isLoading = false

    /// Non-nil when a network or decoding error occurred.
    var error: String? = nil

    // MARK: - Filter State

    /// Category filter. `nil` means show all categories.
    /// Possible values: "imported", "podcasts".
    var selectedCategory: String? = nil

    /// Document type filter. `nil` means show all types.
    /// Possible values: "web", "txt", "pdf", "epub".
    var selectedType: String? = nil

    /// Full-text search query. Empty string means no search filter.
    var searchQuery = ""

    /// Sort field. Default is "created_at" for newest-first display.
    /// Other valid values: "title", "progress".
    var sortBy = "created_at"

    /// Sort direction. "desc" for newest first, "asc" for oldest first.
    var sortOrder = "desc"

    // MARK: - Computed Properties

    /// Whether there are more pages to load (for infinite scroll trigger).
    var hasMorePages: Bool { currentPage < totalPages }

    // MARK: - Dependencies

    /// Service layer for library CRUD operations.
    private let libraryService = LibraryService()

    /// Number of items per page. Matches the web app's default.
    private let pageSize = 20

    // MARK: - Public API

    /// Load the first page of items with the current filter/sort state.
    ///
    /// Replaces the existing `items` array (unlike `loadNextPage()` which appends).
    /// Called on initial view appear and whenever a filter/sort parameter changes.
    func loadItems() async {
        isLoading = true
        error = nil
        currentPage = 1

        do {
            let response = try await libraryService.fetchItems(
                page: 1,
                pageSize: pageSize,
                category: selectedCategory,
                type: selectedType,
                search: searchQuery.isEmpty ? nil : searchQuery,
                sortBy: sortBy,
                sortOrder: sortOrder
            )

            items = response.items
            totalItems = response.total
            totalPages = response.totalPages
        } catch {
            self.error = "Failed to load library: \(error.localizedDescription)"
        }

        isLoading = false
    }

    /// Load the next page and append results to the existing `items` array.
    ///
    /// Called when the user scrolls to the bottom of the list (infinite scroll).
    /// No-op if already loading or if there are no more pages.
    func loadNextPage() async {
        guard !isLoading && hasMorePages else { return }

        isLoading = true
        let nextPage = currentPage + 1

        do {
            let response = try await libraryService.fetchItems(
                page: nextPage,
                pageSize: pageSize,
                category: selectedCategory,
                type: selectedType,
                search: searchQuery.isEmpty ? nil : searchQuery,
                sortBy: sortBy,
                sortOrder: sortOrder
            )

            // Append (not replace) for infinite scroll behavior
            items.append(contentsOf: response.items)
            currentPage = nextPage
            totalItems = response.total
            totalPages = response.totalPages
        } catch {
            self.error = "Failed to load more items: \(error.localizedDescription)"
        }

        isLoading = false
    }

    /// Search the library with the given query.
    ///
    /// Updates `searchQuery` and reloads from page 1. If the query is empty,
    /// this is equivalent to clearing the search filter.
    ///
    /// - Parameter query: The search text to filter by.
    func search(_ query: String) async {
        searchQuery = query
        await loadItems()
    }

    /// Delete a library item from both the backend and the local list.
    ///
    /// The item is removed from `items` optimistically (before the network call
    /// completes) for a snappy UI. If the backend call fails, the item is
    /// re-inserted at its original position and an error is shown.
    ///
    /// - Parameter id: The ID of the item to delete.
    /// - Throws: If the backend delete request fails.
    func deleteItem(id: String) async throws {
        // Optimistic removal: find the item's index and remove it locally
        let removedIndex = items.firstIndex(where: { $0.id == id })
        let removedItem = removedIndex.flatMap { items[$0] }

        if let removedIndex {
            items.remove(at: removedIndex)
            totalItems = max(0, totalItems - 1)
        }

        do {
            try await libraryService.deleteItem(id: id)
        } catch {
            // Rollback: re-insert the item at its original position
            if let removedItem, let removedIndex {
                items.insert(removedItem, at: min(removedIndex, items.count))
                totalItems += 1
            }
            throw error
        }
    }

    /// Pull-to-refresh: reload from page 1 with current filters.
    ///
    /// Identical to `loadItems()` but semantically named for the SwiftUI
    /// `.refreshable` modifier.
    func refresh() async {
        await loadItems()
    }
}
