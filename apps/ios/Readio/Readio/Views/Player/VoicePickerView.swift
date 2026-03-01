import SwiftUI

// MARK: - VoicePickerView
/// A voice selection sheet for choosing the TTS voice used during playback.
///
/// **Layout:**
/// ```
/// ┌──────────────────────────────────────┐
/// │  Choose Voice                        │
/// ├──────────────────────────────────────┤
/// │  [All] [English] [Chinese]           │  ← Language filter tabs
/// ├──────────────────────────────────────┤
/// │  ★ Default Voice                  ✓  │  ← Default option (always first)
/// ├──────────────────────────────────────┤
/// │  ♂ English Trustworthy Man        ✓  │  ← Voice rows
/// │    en · Male                         │
/// │  ♀ English Gentle Woman              │
/// │    en · Female                       │
/// │  ♂ Chinese(Mandarin) Deep Man        │
/// │    zh · Male                         │
/// │  ...                                 │
/// └──────────────────────────────────────┘
/// ```
///
/// **Features:**
/// - **Language filter tabs:** Filter voices by language (All, English, Chinese).
/// - **Default option:** Selecting "Default" clears the voice override, using the
///   server's configured default voice.
/// - **Voice metadata:** Each row shows the voice label, language badge, gender icon,
///   and optional description.
/// - **Selection indicator:** The currently selected voice shows a checkmark.
///
/// **Voice ID Mapping:**
/// - `nil` = Use the default voice (server-configured).
/// - A specific `voice_id` string = Override with that voice.
struct VoicePickerView: View {

    // MARK: - Properties

    /// All available TTS voices from the server.
    let voices: [VoiceInfo]

    /// The currently selected voice ID. `nil` means "use default".
    let selectedVoiceId: String?

    /// The server's default voice ID, shown in the "Default" row's subtitle.
    let defaultVoiceId: String

    /// Callback when the user selects a voice. Pass `nil` for the default.
    let onSelect: (String?) -> Void

    // MARK: - State

    /// The active language filter. `nil` means "All languages".
    @State private var languageFilter: String? = nil

    /// Search text for filtering voices by name.
    @State private var searchText: String = ""

    // MARK: - Computed Properties

    /// Voices filtered by the selected language and search text.
    private var filteredVoices: [VoiceInfo] {
        voices.filter { voice in
            // Language filter
            let matchesLanguage = languageFilter == nil || voice.language == languageFilter

            // Search filter — matches against label and description
            let matchesSearch = searchText.isEmpty ||
                voice.label.localizedCaseInsensitiveContains(searchText) ||
                (voice.description?.localizedCaseInsensitiveContains(searchText) ?? false)

            return matchesLanguage && matchesSearch
        }
    }

    /// The label of the default voice, found by matching `defaultVoiceId` in the voice list.
    private var defaultVoiceLabel: String {
        voices.first { $0.voiceId == defaultVoiceId }?.label ?? defaultVoiceId
    }

    // MARK: - Body

    var body: some View {
        VStack(spacing: 0) {
            // MARK: Language Filter Tabs
            languageFilterTabs

            // MARK: Voice List
            List {
                // MARK: Default Voice Option
                // Always shown at the top, regardless of language filter.
                defaultVoiceRow

                // MARK: Voice Rows
                // Filtered by language and search text.
                Section {
                    ForEach(filteredVoices) { voice in
                        voiceRow(voice)
                    }
                } header: {
                    Text("\(filteredVoices.count) voices")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .listStyle(.insetGrouped)
            .searchable(text: $searchText, prompt: "Search voices")
        }
    }

    // MARK: - Language Filter Tabs

    /// Horizontal filter tabs for selecting the language group.
    ///
    /// Shows "All", "English", and "Chinese" options. The active tab is highlighted
    /// with the accent color. Tapping a tab filters the voice list immediately.
    private var languageFilterTabs: some View {
        HStack(spacing: 8) {
            LanguageFilterChip(
                title: "All",
                isSelected: languageFilter == nil,
                action: { languageFilter = nil }
            )

            LanguageFilterChip(
                title: "English",
                isSelected: languageFilter == "en",
                action: { languageFilter = "en" }
            )

            LanguageFilterChip(
                title: "Chinese",
                isSelected: languageFilter == "zh",
                action: { languageFilter = "zh" }
            )

            Spacer()
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
    }

    // MARK: - Default Voice Row

    /// The "Default" voice option, always shown at the top of the list.
    ///
    /// Selecting this clears the per-item voice override, falling back to the
    /// server's globally configured default voice.
    private var defaultVoiceRow: some View {
        Button {
            onSelect(nil)
        } label: {
            HStack(spacing: 12) {
                // Star icon indicating this is the recommended/default option
                Image(systemName: "star.fill")
                    .font(.title3)
                    .foregroundStyle(.orange)
                    .frame(width: 32)

                VStack(alignment: .leading, spacing: 2) {
                    Text("Default Voice")
                        .font(.subheadline)
                        .fontWeight(.medium)
                        .foregroundStyle(.primary)

                    Text(defaultVoiceLabel)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                Spacer()

                // Checkmark for selection state
                if selectedVoiceId == nil {
                    Image(systemName: "checkmark.circle.fill")
                        .font(.title3)
                        .foregroundStyle(.accentColor)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    // MARK: - Voice Row

    /// Renders a single voice option in the list.
    ///
    /// Shows the voice label, language badge, gender icon, optional description,
    /// and a checkmark if this voice is currently selected.
    private func voiceRow(_ voice: VoiceInfo) -> some View {
        Button {
            onSelect(voice.voiceId)
        } label: {
            HStack(spacing: 12) {
                // Gender icon
                Image(systemName: genderIcon(for: voice.gender))
                    .font(.title3)
                    .foregroundStyle(genderColor(for: voice.gender))
                    .frame(width: 32)

                VStack(alignment: .leading, spacing: 2) {
                    // Voice label (display name)
                    Text(voice.label)
                        .font(.subheadline)
                        .fontWeight(.medium)
                        .foregroundStyle(.primary)
                        .lineLimit(1)

                    // Metadata row: language badge + gender text
                    HStack(spacing: 6) {
                        // Language badge
                        Text(languageDisplayName(voice.language))
                            .font(.system(size: 10, weight: .medium, design: .rounded))
                            .foregroundStyle(.white)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 1)
                            .background(
                                Capsule()
                                    .fill(languageBadgeColor(voice.language))
                            )

                        // Gender label
                        if let gender = voice.gender {
                            Text(gender.capitalized)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }

                    // Optional description
                    if let description = voice.description, !description.isEmpty {
                        Text(description)
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                            .lineLimit(2)
                    }
                }

                Spacer()

                // Selection checkmark
                if selectedVoiceId == voice.voiceId {
                    Image(systemName: "checkmark.circle.fill")
                        .font(.title3)
                        .foregroundStyle(.accentColor)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    // MARK: - Helpers

    /// Returns an SF Symbol name representing the voice's gender.
    private func genderIcon(for gender: String?) -> String {
        switch gender?.lowercased() {
        case "male": return "person.fill"
        case "female": return "person.fill"
        default: return "person.fill.questionmark"
        }
    }

    /// Returns a tint color for the gender icon.
    private func genderColor(for gender: String?) -> Color {
        switch gender?.lowercased() {
        case "male": return .blue
        case "female": return .pink
        default: return .gray
        }
    }

    /// Maps language codes to human-readable names for display.
    private func languageDisplayName(_ code: String) -> String {
        switch code.lowercased() {
        case "en": return "EN"
        case "zh": return "ZH"
        default: return code.uppercased()
        }
    }

    /// Returns the badge background color for a language code.
    private func languageBadgeColor(_ code: String) -> Color {
        switch code.lowercased() {
        case "en": return .blue
        case "zh": return .red
        default: return .gray
        }
    }
}

// MARK: - LanguageFilterChip
/// A small filter chip for the language filter tabs in VoicePickerView.
private struct LanguageFilterChip: View {
    let title: String
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.subheadline)
                .fontWeight(isSelected ? .semibold : .medium)
                .foregroundStyle(isSelected ? .white : .primary)
                .padding(.horizontal, 14)
                .padding(.vertical, 6)
                .background(
                    Capsule()
                        .fill(isSelected ? Color.accentColor : Color(.secondarySystemGroupedBackground))
                )
        }
        .buttonStyle(.plain)
        .animation(.easeInOut(duration: 0.15), value: isSelected)
    }
}

// MARK: - Preview

#Preview {
    NavigationStack {
        VoicePickerView(
            voices: [
                VoiceInfo(
                    voiceId: "English_Trustworthy_Man",
                    label: "Trustworthy Man",
                    language: "en",
                    gender: "male",
                    description: "A calm, authoritative male voice"
                ),
                VoiceInfo(
                    voiceId: "English_Gentle_Woman",
                    label: "Gentle Woman",
                    language: "en",
                    gender: "female",
                    description: "A soft, warm female voice"
                ),
                VoiceInfo(
                    voiceId: "Chinese_Mandarin_Deep_Man",
                    label: "Deep Man",
                    language: "zh",
                    gender: "male",
                    description: nil
                ),
            ],
            selectedVoiceId: "English_Trustworthy_Man",
            defaultVoiceId: "English_Trustworthy_Man",
            onSelect: { id in print("Selected: \(String(describing: id))") }
        )
        .navigationTitle("Choose Voice")
    }
}
