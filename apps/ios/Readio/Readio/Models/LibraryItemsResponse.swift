import Foundation

// MARK: - LibraryItemsResponse
/// Paginated response from the library list endpoint `GET /api/library/items`.
///
/// The backend uses cursor-free offset pagination:
/// - `page` is 1-based (first page = 1)
/// - `page_size` defaults to 20, max 100
/// - `total_pages` is ceil(total / page_size)
///
/// Example usage in a SwiftUI view model:
/// ```swift
/// let response = try await libraryService.fetchItems(page: 1, pageSize: 20)
/// self.items = response.items
/// self.hasMore = response.page < response.totalPages
/// ```
struct LibraryItemsResponse: Codable, Sendable {
    /// Array of library item summaries (without content) for the current page.
    let items: [LibraryItemSummary]

    /// Total number of items matching the current filter criteria across all pages.
    let total: Int

    /// Current page number (1-based). Matches the `page` query parameter.
    let page: Int

    /// Number of items per page. Matches the `page_size` query parameter.
    let pageSize: Int

    /// Total number of pages available: ceil(total / pageSize).
    let totalPages: Int

    // MARK: - CodingKeys
    enum CodingKeys: String, CodingKey {
        case items
        case total
        case page
        case pageSize = "page_size"
        case totalPages = "total_pages"
    }
}
