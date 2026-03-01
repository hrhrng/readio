import SwiftUI

// MARK: - ReaderView
/// The full-screen reader/player view for consuming a library item.
///
/// **Layout:**
/// ```
/// ┌──────────────────────────────────────┐
/// │  ← Back    Book Title     ☰ TOC     │  ← Top bar (ReaderTopBar)
/// ├──────────────────────────────────────┤
/// │                                      │
/// │   Content Area                       │
/// │   (switches based on item type)      │
/// │                                      │
/// │   EPUB → EpubContentView             │
/// │   TXT/Web/PDF → PlainTextContentView │
/// │                                      │
/// │                                      │
/// │                                      │
/// ├──────────────────────────────────────┤
/// │  ◀◀  ▶/⏸  ▶▶  ━━━━●━━━━  1.0x 🎤   │  ← PlayerBarView
/// └──────────────────────────────────────┘
/// ```
///
/// **iPad vs iPhone TOC:**
/// - **iPad (regular width):** TOC appears as a side panel to the right of the
///   content area, using an HStack split.
/// - **iPhone (compact width):** TOC appears as a `.sheet` presentation.
///
/// **Lifecycle:**
/// 1. `onAppear` / `.task`: Load item details, extract sentences, setup TTS engine.
/// 2. During reading: Auto-scroll follows TTS playback, user can tap sentences.
/// 3. `onDisappear`: Save progress to server, cleanup TTS engine resources.
/// 4. Scene phase changes: `readioSaveProgress` notification triggers progress save.
struct ReaderView: View {

    // MARK: - Properties

    /// The ID of the library item to display. Passed from navigation.
    let itemId: String

    // MARK: - State

    /// View model for loading and managing the item's content and reading state.
    @State private var readerVM = ReaderViewModel()

    /// View model for TTS voice selection, speed control, and engine lifecycle.
    @State private var playerVM = PlayerViewModel()

    /// Controls the visibility of the Table of Contents panel/sheet.
    @State private var showTOC = false

    /// Controls the visibility of the voice picker sheet.
    @State private var showVoicePicker = false

    // MARK: - Environment

    /// Current horizontal size class — determines TOC presentation style.
    @Environment(\.horizontalSizeClass) private var sizeClass

    /// Dismiss action for popping this view off the navigation stack.
    @Environment(\.dismiss) private var dismiss

    // MARK: - Body

    var body: some View {
        ZStack {
            if readerVM.isLoading && readerVM.item == nil {
                // MARK: Loading State
                loadingView
            } else if let item = readerVM.item {
                // MARK: Reader Content
                readerContent(for: item)
            } else {
                // MARK: Error State
                errorView
            }
        }
        .navigationBarBackButtonHidden(true) // We provide our own back button in the top bar
        .toolbar(.hidden, for: .navigationBar)
        .toolbar(.hidden, for: .tabBar) // Hide tab bar in reader for immersive experience
        .ignoresSafeArea(.container, edges: .bottom) // Player bar extends to bottom edge
        .task {
            // Load the item and prepare for playback.
            // `.task` cancels automatically if the view disappears mid-load.
            await loadItemAndSetup()
        }
        .onDisappear {
            // Persist reading progress and release TTS resources.
            Task { await readerVM.saveProgress(sentenceIndex: playerVM.engine?.currentSentenceIndex ?? 0) }
            playerVM.cleanup()
        }
        .onReceive(NotificationCenter.default.publisher(for: .readioSaveProgress)) { _ in
            // Save progress when the app is about to lose focus (scene phase change).
            Task { await readerVM.saveProgress(sentenceIndex: playerVM.engine?.currentSentenceIndex ?? 0) }
        }
        .sheet(isPresented: $showVoicePicker) {
            // MARK: Voice Picker Sheet
            NavigationStack {
                VoicePickerView(
                    voices: playerVM.voices,
                    selectedVoiceId: playerVM.currentVoice,
                    defaultVoiceId: playerVM.defaultVoiceId,
                    onSelect: { voiceId in
                        Task { await playerVM.setVoice(voiceId) }
                        showVoicePicker = false
                    }
                )
                .navigationTitle("Choose Voice")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Done") { showVoicePicker = false }
                    }
                }
            }
            .presentationDetents([.medium, .large])
        }
    }

    // MARK: - Reader Content

    /// The main reader layout with top bar, content area, and player bar.
    ///
    /// For iPad, the TOC side panel is shown alongside the content when `showTOC` is true.
    /// For iPhone, the TOC is presented as a sheet.
    @ViewBuilder
    private func readerContent(for item: LibraryItem) -> some View {
        VStack(spacing: 0) {
            // MARK: Top Bar
            // Custom top bar with back button, title, and TOC toggle.
            ReaderTopBar(
                title: item.title,
                onBack: { dismiss() },
                onTOCToggle: { showTOC.toggle() },
                showTOCActive: showTOC
            )

            // MARK: Content + TOC Layout
            if sizeClass == .regular && showTOC {
                // iPad: side-by-side layout with TOC panel on the right
                HStack(spacing: 0) {
                    contentArea(for: item)

                    Divider()

                    // TOC side panel — fixed width
                    TOCView(
                        entries: readerVM.tocEntries,
                        currentChapterId: readerVM.currentChapterId,
                        onSelectChapter: { chapterId in
                            Task { await readerVM.selectChapter(id: chapterId) }
                        }
                    )
                    .frame(width: 280)
                    .transition(.move(edge: .trailing))
                }
            } else {
                // iPhone or iPad without TOC: full-width content
                contentArea(for: item)
            }

            // MARK: Player Bar
            // Only shown when the TTS engine is ready (sentences have been extracted
            // and the engine has been created). The engine is optional on the ViewModel.
            if let engine = playerVM.engine, !readerVM.sentences.isEmpty {
                PlayerBarView(
                    engine: engine,
                    currentSpeed: playerVM.currentSpeed,
                    onSpeedChange: { newSpeed in
                        Task { await playerVM.setSpeed(newSpeed) }
                    },
                    onVoiceTap: { showVoicePicker = true }
                )
            }
        }
        // iPhone: TOC as a sheet
        .sheet(isPresented: Binding(
            get: { showTOC && sizeClass != .regular },
            set: { showTOC = $0 }
        )) {
            NavigationStack {
                TOCView(
                    entries: readerVM.tocEntries,
                    currentChapterId: readerVM.currentChapterId,
                    onSelectChapter: { chapterId in
                        showTOC = false
                        Task { await readerVM.selectChapter(id: chapterId) }
                    }
                )
                .navigationTitle("Table of Contents")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Done") { showTOC = false }
                    }
                }
            }
            .presentationDetents([.medium, .large])
        }
    }

    // MARK: - Content Area

    /// Renders the appropriate content view based on the item's document type.
    ///
    /// - **EPUB:** Uses `EpubContentView` with chapter navigation and rich content blocks.
    /// - **TXT / Web / PDF:** Uses `PlainTextContentView` with simple sentence display.
    @ViewBuilder
    private func contentArea(for item: LibraryItem) -> some View {
        // The engine is optional — use safe defaults (index -1, progress 0)
        // when the engine hasn't been created yet (e.g., during initial loading).
        let sentenceIndex = playerVM.engine?.currentSentenceIndex ?? -1
        let wordProgress = playerVM.engine?.currentWordProgress ?? 0

        switch item.type {
        case .epub:
            EpubContentView(
                chapters: readerVM.chapters,
                currentChapterId: readerVM.currentChapterId,
                currentSentenceIndex: sentenceIndex,
                currentWordProgress: wordProgress,
                onSentenceClick: { index in
                    playerVM.engine?.playFromSentence(index)
                },
                onChapterChange: { chapterId in
                    Task { await readerVM.selectChapter(id: chapterId) }
                }
            )

        case .txt, .web, .pdf:
            PlainTextContentView(
                sentences: readerVM.sentences,
                currentSentenceIndex: sentenceIndex,
                currentWordProgress: wordProgress,
                onSentenceClick: { index in
                    playerVM.engine?.playFromSentence(index)
                }
            )
        }
    }

    // MARK: - Loading View

    /// Full-screen loading indicator shown while the item data is being fetched.
    private var loadingView: some View {
        VStack(spacing: 16) {
            ProgressView()
                .controlSize(.large)
            Text("Loading...")
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemBackground))
    }

    // MARK: - Error View

    /// Error state shown when the item fails to load.
    private var errorView: some View {
        EmptyStateView(
            icon: "exclamationmark.triangle",
            title: "Failed to Load",
            message: "The item could not be loaded. Please check your connection and try again.",
            actionLabel: "Go Back"
        ) {
            dismiss()
        }
    }

    // MARK: - Setup

    /// Loads the item data, extracts sentences for TTS, and configures the player engine.
    ///
    /// **Sequence:**
    /// 1. Load full item details from the server.
    /// 2. Extract sentences from the content (for TTS granularity).
    ///    Note: `extractSentences()` is synchronous — it processes the already-loaded content.
    /// 3. Setup the TTS engine with the item's voice/speed preferences.
    /// 4. Load available voices for the voice picker.
    private func loadItemAndSetup() async {
        // Step 1: Load the full item (with content)
        await readerVM.loadItem(id: itemId)

        // Step 2: Extract sentences for TTS playback (synchronous, operates on loaded content)
        readerVM.extractSentences()

        // Step 3: Setup the TTS playback engine.
        // PlayerViewModel.setupEngine is synchronous — it creates the engine instance.
        if let item = readerVM.item {
            playerVM.setupEngine(
                itemId: item.id,
                sentences: readerVM.sentences,
                speed: item.speed,
                voice: item.voice,
                title: item.title
            )
        }

        // Step 4: Load voice options from the server
        await playerVM.loadVoices()
    }
}

// MARK: - ReaderTopBar
/// Custom top bar for the reader view with back button, title, and TOC toggle.
///
/// We use a custom bar instead of the navigation bar because:
/// 1. The reader is a full-screen immersive experience.
/// 2. We need custom actions (TOC toggle) that don't fit standard toolbar items well.
/// 3. We want precise control over the title truncation and layout.
struct ReaderTopBar: View {

    /// The item title displayed in the center of the bar.
    let title: String

    /// Action triggered when the back button is tapped.
    let onBack: () -> Void

    /// Action triggered when the TOC button is tapped.
    let onTOCToggle: () -> Void

    /// Whether the TOC panel/sheet is currently visible (affects button appearance).
    let showTOCActive: Bool

    var body: some View {
        HStack(spacing: 12) {
            // MARK: Back Button
            Button(action: onBack) {
                HStack(spacing: 4) {
                    Image(systemName: "chevron.left")
                        .font(.body.weight(.semibold))
                    Text("Back")
                        .font(.body)
                }
            }

            Spacer()

            // MARK: Title
            Text(title)
                .font(.headline)
                .lineLimit(1)
                .truncationMode(.middle)

            Spacer()

            // MARK: TOC Toggle Button
            Button(action: onTOCToggle) {
                Image(systemName: showTOCActive ? "list.bullet.circle.fill" : "list.bullet.circle")
                    .font(.title3)
                    .foregroundStyle(showTOCActive ? .accentColor : .primary)
            }
        }
        .padding(.horizontal)
        .padding(.vertical, 10)
        .background(
            Color(.systemBackground)
                .shadow(color: .black.opacity(0.05), radius: 2, y: 1)
        )
    }
}

// MARK: - Preview

#Preview {
    NavigationStack {
        ReaderView(itemId: "test-item-id")
    }
}
