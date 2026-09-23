import SwiftUI

/// Quiet, fixed desktop navigation that keeps the book collection visually
/// dominant. The custom rows intentionally avoid the platform List tint so the
/// committed light palette renders identically on iPad and macOS.
struct SidebarView: View {
    @Environment(\.readioTheme) private var theme
    @Environment(\.locale) private var locale
    @Binding var selection: AppleAppDestination?

    private var primaryDestinations: [AppleAppDestination] {
        AppleAppDestination.allCases.filter { $0 != .settings }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            brand

            VStack(spacing: 8) {
                ForEach(primaryDestinations) { destination in
                    navigationRow(destination)
                }
            }

            Spacer(minLength: 32)
            navigationRow(.settings)
        }
        .padding(.horizontal, 22)
        .padding(.top, 28)
        .padding(.bottom, 24)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(theme.sidebar.ignoresSafeArea())
        .overlay(alignment: .trailing) {
            Rectangle()
                .fill(theme.border)
                .frame(width: 1)
                .ignoresSafeArea()
        }
        .navigationSplitViewColumnWidth(
            min: 220,
            ideal: ReadioAppleLayout.sidebarIdealWidth,
            max: 280
        )
        .navigationTitle("")
    }

    private var brand: some View {
        HStack(spacing: 10) {
            Text("❯_")
                .font(.system(size: 21, weight: .bold, design: .monospaced))
                .foregroundStyle(theme.accent)

            Text("readio")
                .font(.system(size: 22, weight: .semibold, design: .rounded))
                .foregroundStyle(theme.sidebarInk)
        }
        .padding(.horizontal, 12)
        .padding(.bottom, 48)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Readio")
    }

    private func navigationRow(_ destination: AppleAppDestination) -> some View {
        let isSelected = selection == destination

        return Button {
            selection = destination
        } label: {
            HStack(spacing: 14) {
                Image(systemName: destination.icon)
                    .font(.system(size: 17, weight: .medium))
                    .foregroundStyle(isSelected ? theme.accent : theme.sidebarMuted)
                    .frame(width: 22)

                Text(destination.localizedTitle(locale: locale))
                    .font(.system(size: 16, weight: isSelected ? .semibold : .regular))
                    .foregroundStyle(isSelected ? theme.sidebarInk : theme.sidebarMuted)

                Spacer(minLength: 0)
            }
            .padding(.horizontal, 14)
            .frame(height: 48)
            .contentShape(Rectangle())
            .background {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(isSelected ? theme.sidebarSelected : Color.clear)
            }
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }
}

#Preview {
    NavigationSplitView {
        SidebarView(selection: .constant(.library))
    } detail: {
        Text("Library")
    }
}
