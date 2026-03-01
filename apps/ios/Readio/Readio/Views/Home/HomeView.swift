import SwiftUI

// MARK: - HomeView
/// The dashboard / home screen of the Readio app.
///
/// **Layout:**
/// ```
/// ┌──────────────────────────────────────┐
/// │  Readio                  (nav title) │
/// ├──────────────────────────────────────┤
/// │  Welcome message / greeting          │
/// │                                      │
/// │  Continue Reading ──────── See All > │
/// │  ┌─────┐ ┌─────┐ ┌─────┐           │
/// │  │ 📖  │ │ 📖  │ │ 📖  │  ← scroll │
/// │  └─────┘ └─────┘ └─────┘           │
/// │                                      │
/// │  Recently Added ────────── See All > │
/// │  ┌─────┐ ┌─────┐ ┌─────┐           │
/// │  │ 📖  │ │ 📖  │ │ 📖  │  ← scroll │
/// │  └─────┘ └─────┘ └─────┘           │
/// └──────────────────────────────────────┘
/// ```
///
/// **Data Flow:**
/// - Uses `HomeViewModel` to fetch dashboard data (in-progress + recent items).
/// - Pull-to-refresh reloads the dashboard.
/// - Tapping a book card navigates to `ReaderView` via `NavigationLink`.
///
/// **Empty States:**
/// - If no items at all, shows a full-screen empty state with import suggestion.
/// - If only one section is empty, that section is hidden rather than showing
///   an awkward empty carousel.
struct HomeView: View {

    // MARK: - State

    /// The view model that fetches and holds dashboard data (in-progress, recent items).
    @State private var viewModel = HomeViewModel()

    // MARK: - Body

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                // MARK: Welcome Header
                welcomeHeader

                if viewModel.isLoading && viewModel.inProgressItems.isEmpty && viewModel.recentItems.isEmpty {
                    // MARK: Loading Skeleton
                    loadingPlaceholder
                } else if viewModel.inProgressItems.isEmpty && viewModel.recentItems.isEmpty {
                    // MARK: Empty State
                    emptyDashboard
                } else {
                    // MARK: Continue Reading Section
                    // Only show if there are items with 0 < progress < 100
                    if !viewModel.inProgressItems.isEmpty {
                        SectionCarouselView(
                            title: "Continue Reading",
                            items: viewModel.inProgressItems
                        )
                    }

                    // MARK: Recently Added Section
                    // Shows the most recently imported/created items
                    if !viewModel.recentItems.isEmpty {
                        SectionCarouselView(
                            title: "Recently Added",
                            items: viewModel.recentItems
                        )
                    }
                }
            }
            .padding(.vertical)
        }
        .refreshable {
            // Pull-to-refresh triggers a full dashboard reload.
            // The `refreshable` modifier automatically shows and hides the spinner.
            await viewModel.loadDashboard()
        }
        .navigationTitle("Readio")
        .task {
            // Load dashboard data when the view first appears.
            // `.task` is preferred over `.onAppear` because it automatically
            // cancels the async work if the view disappears before completion.
            await viewModel.loadDashboard()
        }
    }

    // MARK: - Welcome Header

    /// A friendly greeting that changes based on the time of day.
    /// Provides a warm, personalized feel to the home screen.
    private var welcomeHeader: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(greetingText)
                .font(.title2)
                .fontWeight(.semibold)
                .foregroundStyle(.primary)

            Text("What would you like to read today?")
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal)
    }

    /// Generates a time-appropriate greeting string.
    /// - Before noon: "Good Morning"
    /// - Noon to 5 PM: "Good Afternoon"
    /// - After 5 PM: "Good Evening"
    private var greetingText: String {
        let hour = Calendar.current.component(.hour, from: Date())
        switch hour {
        case 0..<12:
            return "Good Morning"
        case 12..<17:
            return "Good Afternoon"
        default:
            return "Good Evening"
        }
    }

    // MARK: - Loading Placeholder

    /// Shimmer-like placeholder shown while the dashboard is loading.
    /// Uses redacted modifier for a native loading appearance.
    private var loadingPlaceholder: some View {
        VStack(alignment: .leading, spacing: 24) {
            // Fake carousel section
            ForEach(0..<2, id: \.self) { _ in
                VStack(alignment: .leading, spacing: 12) {
                    Text("Section Title")
                        .font(.title3)
                        .fontWeight(.bold)
                        .padding(.horizontal)

                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 14) {
                            ForEach(0..<4, id: \.self) { _ in
                                RoundedRectangle(cornerRadius: 12)
                                    .fill(.quaternary)
                                    .frame(width: 160, height: 220)
                            }
                        }
                        .padding(.horizontal)
                    }
                }
            }
        }
        .redacted(reason: .placeholder)
    }

    // MARK: - Empty Dashboard

    /// Full-screen empty state shown when the library has no items.
    /// Encourages the user to import their first book.
    private var emptyDashboard: some View {
        EmptyStateView(
            icon: "book.closed.fill",
            title: "Your Library is Empty",
            message: "Import an EPUB, PDF, or TXT file, or paste a URL to get started with Readio.",
            actionLabel: "Import Now"
        ) {
            // This action could navigate to the Import tab.
            // For now, we use NotificationCenter or a shared navigation state.
        }
        .padding(.top, 40)
    }
}

// MARK: - Preview

#Preview {
    NavigationStack {
        HomeView()
    }
}
