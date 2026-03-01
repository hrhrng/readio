import SwiftUI

// MARK: - EmptyStateView
/// A reusable empty state component displayed when a list or section has no content.
///
/// **Layout:**
/// ```
/// ┌──────────────────────────────────────┐
/// │                                      │
/// │            ╭───────────╮             │
/// │            │  SF Icon  │             │
/// │            ╰───────────╯             │
/// │                                      │
/// │         Title Goes Here              │
/// │    A longer description message      │
/// │    explaining what to do next.       │
/// │                                      │
/// │          [ Action Button ]           │  ← Optional
/// │                                      │
/// └──────────────────────────────────────┘
/// ```
///
/// **Usage Examples:**
/// ```swift
/// // Basic empty state (no action button)
/// EmptyStateView(
///     icon: "magnifyingglass",
///     title: "No Results",
///     message: "Try a different search term."
/// )
///
/// // With action button
/// EmptyStateView(
///     icon: "book.closed.fill",
///     title: "Library is Empty",
///     message: "Import a file to get started.",
///     actionLabel: "Import Now"
/// ) {
///     // Navigate to import screen
/// }
/// ```
///
/// **Design Notes:**
/// - Centers content both horizontally and vertically within its container.
/// - Uses system colors for automatic light/dark mode support.
/// - The icon is rendered inside a circle background for visual emphasis.
/// - The action button uses a bordered prominent style for high visibility.
struct EmptyStateView: View {

    // MARK: - Properties

    /// SF Symbol name for the illustration icon (e.g., "book.closed.fill").
    let icon: String

    /// The bold headline text (e.g., "No Results", "Library is Empty").
    let title: String

    /// A descriptive message explaining the empty state and suggesting next steps.
    let message: String

    /// Optional closure executed when the action button is tapped.
    /// When nil, no action button is rendered.
    var action: (() -> Void)? = nil

    /// Label text for the optional action button (e.g., "Import Now", "Try Again").
    /// Required if `action` is non-nil; ignored if `action` is nil.
    var actionLabel: String? = nil

    // MARK: - Body

    var body: some View {
        VStack(spacing: 16) {
            // MARK: Icon
            // Large SF Symbol icon inside a circular background.
            // The circle provides visual weight and makes the icon a focal point.
            Image(systemName: icon)
                .font(.system(size: 36))
                .foregroundStyle(.secondary)
                .frame(width: 72, height: 72)
                .background(
                    Circle()
                        .fill(Color(.secondarySystemGroupedBackground))
                )

            // MARK: Title
            Text(title)
                .font(.title3)
                .fontWeight(.semibold)
                .foregroundStyle(.primary)
                .multilineTextAlignment(.center)

            // MARK: Message
            Text(message)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .lineLimit(3)
                .frame(maxWidth: 280) // Constrain width for readable line lengths

            // MARK: Action Button (Optional)
            // Only shown when both `action` and `actionLabel` are provided.
            if let action, let actionLabel {
                Button(action: action) {
                    Text(actionLabel)
                        .fontWeight(.medium)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.regular)
                .padding(.top, 4)
            }
        }
        .padding(32)
        .frame(maxWidth: .infinity) // Centers horizontally within the parent
    }
}

// MARK: - Preview

#Preview("With Action") {
    EmptyStateView(
        icon: "book.closed.fill",
        title: "Your Library is Empty",
        message: "Import an EPUB, PDF, or TXT file to start reading with Readio.",
        actionLabel: "Import Now"
    ) {
        print("Import tapped")
    }
}

#Preview("Without Action") {
    EmptyStateView(
        icon: "magnifyingglass",
        title: "No Results Found",
        message: "No items match your search query. Try a different term."
    )
}

#Preview("Network Error") {
    EmptyStateView(
        icon: "wifi.exclamationmark",
        title: "Connection Error",
        message: "Could not connect to the Readio server. Check your network settings.",
        actionLabel: "Retry"
    ) {
        print("Retry tapped")
    }
}
