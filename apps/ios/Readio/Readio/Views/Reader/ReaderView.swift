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
/// │   Reader IR → semantic block view    │
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
    private let onBack: (() -> Void)?

    // MARK: - State

    /// View model for loading and managing the item's content and reading state.
    @State private var readerVM = ReaderViewModel()

    /// View model for TTS voice selection, speed control, and engine lifecycle.
    @State private var playerVM: PlayerViewModel

    /// Controls the visibility of the Table of Contents panel/sheet.
    @State private var showTOC = false

    /// Controls the visibility of the voice picker sheet.
    @State private var showVoicePicker = false

    @State private var showBookmarks = false

    @State private var showSearch = false

    @State private var bookmarkBeingRenamed: AppleBookmark? = nil

    @State private var bookmarkRenameDraft = ""

    @State private var readerToolbarPointerInside = false

    @State private var readerPlayerDockPointerInside = false

    // MARK: - Environment

    /// Current horizontal size class — determines TOC presentation style.
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.readioTheme) private var theme

    /// Dismiss action for popping this view off the navigation stack.
    @Environment(\.dismiss) private var dismiss

    init(itemId: String, onBack: (() -> Void)? = nil) {
        self.itemId = itemId
        self.onBack = onBack
        let preferences = AppleTTSPreferences()
        _playerVM = State(
            initialValue: PlayerViewModel(
                ttsProvider: AppleTTSProviderFactory.make(preferences: preferences),
                ttsMode: preferences.mode
            )
        )
    }

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
        .background(theme.canvas.ignoresSafeArea())
        .tint(theme.accent)
        .navigationBarBackButtonHidden(true) // We provide our own back button in the top bar
        .readioReaderPlatformChrome()
        .task {
            // Load the item and prepare for playback.
            // `.task` cancels automatically if the view disappears mid-load.
            await loadItemAndSetup()
        }
        .onDisappear {
            // Read-aloud cleanup emits Rust's sampled persist effect. Manual and
            // Auto have no audio pause cursor, so commit their visible sentence
            // explicitly before releasing the executors.
            let sentence = playerVM.engine?.currentSentenceIndex ?? 0
            if playerVM.readingMode != .readAloud {
                readerVM.enqueueProgressSave(sentenceIndex: sentence)
            }
            playerVM.cleanup()
            Task { await readerVM.flushProgressSaves() }
        }
        .onReceive(NotificationCenter.default.publisher(for: .readioSaveProgress)) { _ in
            // Save progress when the app is about to lose focus (scene phase change).
            readerVM.enqueueProgressSave(
                sentenceIndex: playerVM.engine?.currentSentenceIndex ?? 0
            )
            Task { await readerVM.flushProgressSaves() }
        }
        .onReceive(NotificationCenter.default.publisher(for: .readioReplicaDidSync)) { _ in
            Task { await readerVM.refreshReplicaProjection() }
        }
        .onReceive(NotificationCenter.default.publisher(for: UserDefaults.didChangeNotification)) { _ in
            // A settings window/scene can edit the same local preferences while
            // this reader remains alive. Equivalent refreshes are idempotent.
            playerVM.refreshReadingPreferences()
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
                .readioInlineNavigationTitle()
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Done") { showVoicePicker = false }
                    }
                }
            }
            .presentationDetents([.medium, .large])
        }
        .sheet(isPresented: $showBookmarks) {
            NavigationStack {
                List {
                    if readerVM.bookmarks.isEmpty {
                        ContentUnavailableView(
                            "No Bookmarks",
                            systemImage: "bookmark",
                            description: Text("Save the current sentence from the reader toolbar.")
                        )
                    } else {
                        ForEach(readerVM.bookmarks) { bookmark in
                            Button {
                                jumpToBookmark(bookmark)
                                showBookmarks = false
                            } label: {
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(bookmark.note)
                                        .foregroundStyle(.primary)
                                        .lineLimit(2)
                                    Text("Position \(bookmark.charOffset)")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                            }
                            .swipeActions {
                                Button(role: .destructive) {
                                    Task { await readerVM.deleteBookmark(bookmark) }
                                } label: {
                                    Label("Delete", systemImage: "trash")
                                }
                            }
                            .swipeActions(edge: .leading) {
                                Button {
                                    bookmarkRenameDraft = bookmark.note
                                    bookmarkBeingRenamed = bookmark
                                } label: {
                                    Label("Rename", systemImage: "pencil")
                                }
                                .tint(.accentColor)
                            }
                        }
                    }
                }
                .navigationTitle("Bookmarks")
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { showBookmarks = false }
                    }
                }
            }
            .presentationDetents([.medium, .large])
        }
        .alert(
            "Rename Bookmark",
            isPresented: Binding(
                get: { bookmarkBeingRenamed != nil },
                set: { isPresented in
                    if !isPresented { bookmarkBeingRenamed = nil }
                }
            )
        ) {
            TextField("Passage label", text: $bookmarkRenameDraft)
            Button("Cancel", role: .cancel) {
                bookmarkBeingRenamed = nil
            }
            Button("Save") {
                guard let bookmark = bookmarkBeingRenamed else { return }
                let note = bookmarkRenameDraft
                bookmarkBeingRenamed = nil
                Task { await readerVM.renameBookmark(bookmark, note: note) }
            }
        } message: {
            Text("Leave the label empty to restore the current passage text.")
        }
        .sheet(isPresented: $showSearch) {
            NavigationStack {
                ReaderSearchView(
                    viewModel: readerVM,
                    onActivate: { index in
                        jumpToSearchHit(at: index)
                    },
                    onWalk: { direction in
                        walkSearchHits(direction)
                    }
                )
                .navigationTitle("Find in Book")
                .readioInlineNavigationTitle()
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { showSearch = false }
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
        Group {
            #if os(macOS)
            GeometryReader { geometry in
                ZStack(alignment: .top) {
                    readerBody(for: item, includesPlayer: false)

                    readerTopBar(for: item)
                        .contentShape(Rectangle())
                        .opacity(readerToolbarVisibility == .visible ? 1 : 0)
                        .offset(y: readerToolbarVisibility == .visible ? 0 : -8)
                        .allowsHitTesting(readerToolbarVisibility == .visible)
                        .animation(
                            .easeOut(duration: 0.16),
                            value: readerToolbarVisibility
                        )
                        .zIndex(10)

                    playerBar
                        .padding(.horizontal, 16)
                        .padding(.bottom, 12)
                        .opacity(readerPlayerDockVisibility == .visible ? 1 : 0)
                        .offset(y: readerPlayerDockVisibility == .visible ? 0 : 10)
                        .allowsHitTesting(readerPlayerDockVisibility == .visible)
                        .animation(
                            .easeOut(duration: 0.18),
                            value: readerPlayerDockVisibility
                        )
                        .frame(maxHeight: .infinity, alignment: .bottom)
                        .zIndex(10)
                }
                .frame(width: geometry.size.width, height: geometry.size.height)
                .onContinuousHover { phase in
                    switch phase {
                    case .active(let location):
                        let targets = AppleReaderSurfacePolicy.hoverTargets(
                            locationY: location.y,
                            viewportHeight: geometry.size.height,
                            playerDockVisible: readerPlayerDockPointerInside
                        )
                        readerToolbarPointerInside = targets.toolbar
                        readerPlayerDockPointerInside = targets.playerDock
                    case .ended:
                        readerToolbarPointerInside = false
                        readerPlayerDockPointerInside = false
                    }
                }
            }
            #else
            VStack(spacing: 0) {
                readerTopBar(for: item)
                readerBody(for: item, includesPlayer: true)
            }
            #endif
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
                        Task { await selectChapterAndUpdatePlayer(chapterId) }
                    }
                )
                .navigationTitle("Table of Contents")
                .readioInlineNavigationTitle()
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Done") { showTOC = false }
                    }
                }
            }
            .presentationDetents([.medium, .large])
        }
    }

    private var readerToolbarVisibility: AppleReaderToolbarVisibility {
        AppleReaderSurfacePolicy.toolbarVisibility(
            pointerInside: readerToolbarPointerInside
        )
    }

    private var readerPlayerDockVisibility: AppleReaderToolbarVisibility {
        AppleReaderSurfacePolicy.playerDockVisibility(
            pointerInside: readerPlayerDockPointerInside
        )
    }

    private func readerTopBar(for item: LibraryItem) -> ReaderTopBar {
        ReaderTopBar(
            title: item.title,
            onBack: exitReader,
            onSearch: { showSearch = true },
            bookmarkCount: readerVM.bookmarks.count,
            onAddBookmark: {
                let index = playerVM.engine?.currentSentenceIndex
                    ?? readerVM.restoredSentenceIndex
                Task { await readerVM.addBookmark(sentenceIndex: index) }
            },
            onShowBookmarks: { showBookmarks = true },
            onTOCToggle: { showTOC.toggle() },
            showTOCActive: showTOC
        )
    }

    @ViewBuilder
    private func readerBody(for item: LibraryItem, includesPlayer: Bool) -> some View {
        VStack(spacing: 0) {
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
                            Task { await selectChapterAndUpdatePlayer(chapterId) }
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
            if includesPlayer {
                playerBar
            }
        }
    }

    @ViewBuilder
    private var playerBar: some View {
        if let engine = playerVM.engine, !readerVM.sentences.isEmpty {
            PlayerBarView(
                engine: engine,
                currentSpeed: playerVM.currentSpeed,
                readingMode: playerVM.readingMode,
                readingEffort: playerVM.readingEffort,
                onReadingModeChange: { mode in
                    playerVM.setReadingMode(mode)
                },
                onReadingEffortChange: { effort in
                    playerVM.setReadingEffort(effort)
                },
                onVoiceTap: { showVoicePicker = true }
            )
        }
    }

    // MARK: - Content Area

    /// Renders the appropriate content view based on the item's document type.
    ///
    /// Every document type uses the same Reader IR semantic-block projection.
    @ViewBuilder
    private func contentArea(for item: LibraryItem) -> some View {
        // The engine is optional — use safe defaults (index -1, progress 0)
        // when the engine hasn't been created yet (e.g., during initial loading).
        let sentenceIndex = playerVM.engine?.currentSentenceIndex ?? -1
        let wordRange = playerVM.engine?.currentWordRange

        ReaderDocumentView(
            chapters: readerVM.chapters,
            currentChapterId: readerVM.currentChapterId,
            currentSentenceIndex: sentenceIndex,
            currentWordRange: wordRange,
            onSentenceClick: { index in
                playerVM.engine?.playFromSentence(index)
            },
            onChapterChange: { chapterId in
                Task { await selectChapterAndUpdatePlayer(chapterId) }
            },
            loadImageAsset: item.type == .epub
                ? { assetID in
                    try await readerVM.loadEpubAsset(assetID: assetID)
                }
                : nil
        )
    }

    // MARK: - Loading View

    /// Full-screen loading indicator shown while the item data is being fetched.
    private var loadingView: some View {
        VStack(spacing: 16) {
            ProgressView()
                .controlSize(.large)
            Text("Loading...")
                .font(.subheadline)
                .foregroundStyle(theme.secondaryInk)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(theme.canvas)
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
            exitReader()
        }
    }

    private func exitReader() {
        if let onBack {
            onBack()
        } else {
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
                title: item.title,
                highlightDocument: readerVM.highlightDocument,
                onPersistProgress: { sentence in
                    readerVM.enqueueProgressSave(sentenceIndex: sentence)
                }
            )
            playerVM.engine?.seekToSentence(readerVM.restoredSentenceIndex)
        }

        // Step 4: Load voice options from the server
        await playerVM.loadVoices()
    }

    private func jumpToSearchHit(at index: Int) {
        let previousChapter = readerVM.currentChapterId
        guard let sentence = readerVM.activateSearchHit(at: index) else { return }
        if readerVM.currentChapterId != previousChapter {
            playerVM.updateSentences(
                readerVM.sentences,
                startingAt: sentence
            )
            return
        }
        playerVM.engine?.seekToSentence(sentence)
    }

    private func jumpToBookmark(_ bookmark: AppleBookmark) {
        let previousChapter = readerVM.currentChapterId
        guard let sentence = readerVM.activateBookmark(bookmark) else { return }
        if readerVM.currentChapterId != previousChapter {
            playerVM.updateSentences(
                readerVM.sentences,
                startingAt: sentence
            )
            return
        }
        playerVM.engine?.seekToSentence(sentence)
    }

    private func walkSearchHits(_ direction: AppleSearchWalkDirection) {
        let previousChapter = readerVM.currentChapterId
        guard let sentence = readerVM.walkSearchHits(direction) else { return }
        if readerVM.currentChapterId != previousChapter {
            playerVM.updateSentences(
                readerVM.sentences,
                startingAt: sentence
            )
            return
        }
        playerVM.engine?.seekToSentence(sentence)
    }

    private func selectChapterAndUpdatePlayer(_ chapterId: String) async {
        await readerVM.selectChapter(id: chapterId)
        playerVM.updateSentences(
            readerVM.sentences,
            startingAt: readerVM.restoredSentenceIndex
        )
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
    @Environment(\.readioTheme) private var theme

    /// The item title displayed in the center of the bar.
    let title: String

    /// Action triggered when the back button is tapped.
    let onBack: () -> Void

    let onSearch: () -> Void

    let bookmarkCount: Int

    let onAddBookmark: () -> Void

    let onShowBookmarks: () -> Void

    /// Action triggered when the TOC button is tapped.
    let onTOCToggle: () -> Void

    /// Whether the TOC panel/sheet is currently visible (affects button appearance).
    let showTOCActive: Bool

    var body: some View {
        HStack(spacing: 10) {
            // MARK: Back Button
            Button(action: onBack) {
                HStack(spacing: 4) {
                    Image(systemName: "chevron.left")
                        .font(.body.weight(.semibold))
                    Text("back")
                        .font(.system(size: 14, weight: .medium, design: .monospaced))
                }
            }
            .buttonStyle(.plain)
            .foregroundStyle(theme.accent)

            Spacer()

            // MARK: Title
            HStack(spacing: 8) {
                Text("❯_")
                    .font(.system(size: 14, weight: .bold, design: .monospaced))
                    .foregroundStyle(theme.accent)
                Text(title)
                    .font(.system(size: 15, weight: .semibold))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }

            Spacer()

            Button(action: onSearch) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 16, weight: .medium))
                    .frame(width: 32, height: 32)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Find in book")

            Menu {
                Button("Add Bookmark", systemImage: "bookmark.badge.plus", action: onAddBookmark)
                Button("Show Bookmarks", systemImage: "bookmark", action: onShowBookmarks)
                    .disabled(bookmarkCount == 0)
            } label: {
                Image(systemName: bookmarkCount > 0 ? "bookmark.fill" : "bookmark")
                    .font(.system(size: 16, weight: .medium))
                    .frame(width: 32, height: 32)
                    .overlay(alignment: .topTrailing) {
                        if bookmarkCount > 0 {
                            Text("\(bookmarkCount)")
                                .font(.system(size: 8, weight: .bold))
                                .padding(2)
                                .background(.tint, in: Circle())
                                .foregroundStyle(theme.onAccent)
                                .offset(x: 7, y: -7)
                        }
                    }
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .accessibilityLabel("Bookmarks")

            // MARK: TOC Toggle Button
            Button(action: onTOCToggle) {
                Image(systemName: showTOCActive ? "list.bullet.circle.fill" : "list.bullet.circle")
                    .font(.system(size: 16, weight: .medium))
                    .frame(width: 32, height: 32)
                    .foregroundStyle(showTOCActive ? theme.accent : theme.ink)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Table of contents")
        }
        .padding(.horizontal, 20)
        .frame(height: 52)
        .background(
            Rectangle()
                .fill(theme.canvas)
                .overlay(alignment: .bottom) {
                    Rectangle()
                        .fill(theme.border)
                        .frame(height: 1)
                }
        )
    }
}

private struct ReaderSearchView: View {
    @Bindable var viewModel: ReaderViewModel
    @State private var query: String
    let onActivate: (Int) -> Void
    let onWalk: (AppleSearchWalkDirection) -> Void

    init(
        viewModel: ReaderViewModel,
        onActivate: @escaping (Int) -> Void,
        onWalk: @escaping (AppleSearchWalkDirection) -> Void
    ) {
        self.viewModel = viewModel
        self.onActivate = onActivate
        self.onWalk = onWalk
        _query = State(initialValue: viewModel.searchQuery)
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)
                TextField("Find in this book", text: $query)
                    .textFieldStyle(.plain)
                    .submitLabel(.search)
                    .onSubmit(runSearch)
                if !query.isEmpty {
                    Button {
                        query = ""
                        viewModel.search("")
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundStyle(.secondary)
                    }
                    .accessibilityLabel("Clear search")
                }
                Button("Find", action: runSearch)
                    .buttonStyle(.borderedProminent)
                    .disabled(query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            .padding()

            if let searchError = viewModel.searchError {
                ContentUnavailableView(
                    "Search Unavailable",
                    systemImage: "exclamationmark.magnifyingglass",
                    description: Text(searchError)
                )
            } else if let result = viewModel.searchResult {
                HStack {
                    Text(
                        "\(result.totalOccurrences) matches in \(result.totalPassages) passages"
                    )
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()

                    Spacer()

                    Button {
                        onWalk(.previous)
                    } label: {
                        Image(systemName: "chevron.up")
                    }
                    .accessibilityLabel("Previous match")
                    .disabled(viewModel.searchHits.isEmpty)

                    Button {
                        onWalk(.next)
                    } label: {
                        Image(systemName: "chevron.down")
                    }
                    .accessibilityLabel("Next match")
                    .disabled(viewModel.searchHits.isEmpty)
                }
                .padding(.horizontal)
                .padding(.bottom, 8)

                if viewModel.searchHits.isEmpty {
                    ContentUnavailableView.search(text: viewModel.searchQuery)
                } else {
                    List(Array(viewModel.searchHits.enumerated()), id: \.element.id) { pair in
                        let (index, hit) = pair
                        Button {
                            onActivate(index)
                        } label: {
                            HStack(alignment: .firstTextBaseline, spacing: 10) {
                                Text("\(index + 1)")
                                    .font(.caption.monospacedDigit())
                                    .foregroundStyle(.secondary)
                                    .frame(minWidth: 24, alignment: .trailing)
                                highlightedSnippet(hit)
                                    .foregroundStyle(.primary)
                                    .multilineTextAlignment(.leading)
                                Spacer(minLength: 0)
                            }
                            .contentShape(Rectangle())
                        }
                        .listRowBackground(
                            viewModel.activeSearchHitIndex == index
                                ? Color.accentColor.opacity(0.1)
                                : Color.clear
                        )
                    }
                    .listStyle(.plain)
                }
            } else {
                ContentUnavailableView(
                    "Search This Book",
                    systemImage: "text.magnifyingglass",
                    description: Text(
                        "Search stays on device and uses the shared Rust index."
                    )
                )
            }
        }
    }

    private func runSearch() {
        viewModel.search(query)
    }

    private func highlightedSnippet(_ hit: AppleReaderSearchHit) -> Text {
        let scalars = Array(hit.snippet.unicodeScalars)
        let start = min(Int(hit.snippetMatchRange.first ?? 0), scalars.count)
        let end = min(
            max(start, Int(hit.snippetMatchRange.dropFirst().first ?? 0)),
            scalars.count
        )
        let before = scalars[..<start].map(String.init).joined()
        let match = scalars[start..<end].map(String.init).joined()
        let after = scalars[end...].map(String.init).joined()
        return Text(before)
            + Text(match).bold().foregroundColor(.accentColor)
            + Text(after)
    }
}

private extension View {
    /// iPhone and iPad expose navigation/tab bars that the immersive reader
    /// replaces. Native macOS has neither placement, so its shared reader keeps
    /// the ordinary window toolbar untouched.
    @ViewBuilder
    func readioReaderPlatformChrome() -> some View {
        #if os(iOS)
        toolbar(.hidden, for: .navigationBar)
            .toolbar(.hidden, for: .tabBar)
            .ignoresSafeArea(.container, edges: .bottom)
        #else
        if AppleReaderSurfacePolicy.hidesNativeWindowToolbar {
            toolbar(.hidden, for: .windowToolbar)
        } else {
            self
        }
        #endif
    }

    @ViewBuilder
    func readioInlineNavigationTitle() -> some View {
        #if os(iOS)
        navigationBarTitleDisplayMode(.inline)
        #else
        self
        #endif
    }
}

// MARK: - Preview

#Preview {
    NavigationStack {
        ReaderView(itemId: "test-item-id")
    }
}
