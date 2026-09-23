import AVFoundation
import Foundation
import MediaPlayer

// MARK: - TTSPlaybackEngine

/// Apple effect adapter for the portable Rust playback state machine.
///
/// Rust owns playback state, sentence advancement, generation invalidation,
/// prefetch selection, sleep-deadline decisions, and recovery policy. This type
/// owns only capabilities supplied by Apple: native/cloud synthesis,
/// `AVAudioPlayer`, cache/dedup, clocks, the audio session, and Now Playing.
@MainActor
@Observable
final class TTSPlaybackEngine: ReadioPlaybackPlatform, ReadioTextAdvancePlatform {

    // MARK: - View state

    private(set) var isPlaying = false
    private(set) var isLoading = false
    private(set) var currentSentenceIndex = 0
    private(set) var currentWordProgress: Double = 0
    private(set) var currentWordRange: ReadioScalarRange?
    private(set) var progressBasisPoints: UInt16 = 0
    private(set) var estimatedRemainingSeconds: Double = -1
    private(set) var sleepDeadlineUnixMilliseconds: Int64?
    var ttsError: String?

    // MARK: - Configuration

    private(set) var itemId: String
    private(set) var sentences: [Sentence]
    private var speed: Double
    private var voice: String?
    private var itemTitle: String
    private(set) var readingMode: ReadioReadingMode
    private var baseTextCharactersPerSecond: Double
    private let onPersistProgress: @MainActor (Int) -> Void

    // MARK: - Apple effects

    private var audioPlayer: AVAudioPlayer?
    private var audioPlayerSentence: Int?
    private let cache = AudioCache(capacity: 50)
    private let ttsProvider: any ReadioAppleTTSProvider
    private let sdk = ReadioSDK()
    private var highlightDocument: ReadioHighlightDocument?
    private var observedAudioDurations: [Int: UInt64] = [:]
    private var isCancelled = false
    private var suppressProgressPersistence = false
    private var shouldResumeAfterAudioInterruption = false
    #if os(iOS)
    private var audioSessionObservers: [NSObjectProtocol] = []
    #endif

    private enum AudioFetchResult {
        case success(data: Data, durationMilliseconds: Double)
        case failure(message: String)
    }

    /// One task per sentence lets an urgent Rust fetch effect reuse speculative
    /// native rendering without coupling reducer state to Swift task identity.
    private var fetchTasks = [Int: Task<AudioFetchResult, Never>]()
    private var prefetchTask: Task<Void, Never>?
    private var playbackCompletionTask: Task<Void, Never>?
    private var tickTask: Task<Void, Never>?
    private var progressTimer: Timer?
    private var textAdvanceTask: Task<Void, Never>?

    /// Optional because the executor needs this fully initialized object as its
    /// platform. It is installed before the initializer returns.
    private var playbackExecutor: ReadioPlaybackExecutor?
    private var textAdvanceExecutor: ReadioTextAdvanceExecutor?

    private let silentRetryCount = 2
    private let silentRetryDelayMilliseconds: UInt64 = 800
    private let maxConcurrentPrefetch = 2

    // MARK: - Initialization

    init(
        itemId: String,
        sentences: [Sentence],
        speed: Double,
        voice: String?,
        itemTitle: String,
        highlightDocument: ReadioHighlightDocument? = nil,
        ttsProvider: (any ReadioAppleTTSProvider)? = nil,
        readingMode: ReadioReadingMode = .readAloud,
        textCharactersPerSecond: Double = 46,
        onPersistProgress: @escaping @MainActor (Int) -> Void = { _ in }
    ) {
        self.itemId = itemId
        self.sentences = sentences
        self.speed = speed
        self.voice = voice
        self.itemTitle = itemTitle
        self.highlightDocument = highlightDocument
        self.ttsProvider = ttsProvider ?? AppleNativeTTSProvider()
        self.readingMode = readingMode
        self.baseTextCharactersPerSecond = textCharactersPerSecond / max(speed, 0.01)
        self.onPersistProgress = onPersistProgress

        installPlaybackExecutor(sentenceCount: sentences.count)
        installTextAdvanceExecutor()
        setupAudioSession()
        setupRemoteCommands()
    }

    // MARK: - Public commands

    func play() {
        switch readingMode {
        case .manual:
            return
        case .auto:
            runTextExecutor { try $0.start() }
        case .readAloud:
            runExecutor { try $0.play() }
        }
    }

    func pause() {
        switch readingMode {
        case .manual:
            runTextExecutor { try $0.pause() }
        case .auto:
            runTextExecutor { try $0.pause() }
        case .readAloud:
            runExecutor { try $0.pause() }
        }
    }

    func playFromSentence(_ index: Int) {
        guard sentences.indices.contains(index) else { return }
        switch readingMode {
        case .manual:
            runTextExecutor { try $0.seek(sentence: index, autoplay: false) }
        case .auto:
            runTextExecutor { try $0.seek(sentence: index, autoplay: true) }
        case .readAloud:
            runExecutor { try $0.seek(sentence: index, autoplay: true) }
        }
    }

    func seekToSentence(_ index: Int) {
        guard sentences.indices.contains(index) else { return }
        switch readingMode {
        case .manual:
            runTextExecutor { try $0.seek(sentence: index, autoplay: false) }
        case .auto:
            runTextExecutor { try $0.seek(sentence: index, autoplay: true) }
        case .readAloud:
            runExecutor { try $0.seek(sentence: index, autoplay: false) }
        }
    }

    func nextSentence() {
        if readingMode != .readAloud {
            guard let executor = textAdvanceExecutor else { return }
            let target = executor.state.currentSentence + 1
            guard sentences.indices.contains(target) else { return }
            runTextExecutor {
                try $0.seek(
                    sentence: target,
                    autoplay: executor.state.status == .running
                )
            }
            return
        }
        guard let executor = playbackExecutor else { return }
        let next = executor.state.currentSentence + 1
        guard sentences.indices.contains(next) else { return }
        let autoplay = isActive(executor.state.status)
        runExecutor { try $0.seek(sentence: next, autoplay: autoplay) }
    }

    func prevSentence() {
        if readingMode != .readAloud {
            guard let executor = textAdvanceExecutor else { return }
            runTextExecutor {
                try $0.seek(
                    sentence: max(0, executor.state.currentSentence - 1),
                    autoplay: executor.state.status == .running
                )
            }
            return
        }
        guard let executor = playbackExecutor else { return }
        let restartCurrent = audioPositionMilliseconds > 2_000
        let target = restartCurrent
            ? executor.state.currentSentence
            : max(0, executor.state.currentSentence - 1)
        let autoplay = isActive(executor.state.status)
        runExecutor {
            try $0.seek(
                sentence: target,
                positionMilliseconds: 0,
                autoplay: autoplay
            )
        }
    }

    func updateSpeed(_ newSpeed: Double) {
        guard newSpeed != speed else { return }
        speed = newSpeed
        audioPlayer?.rate = Float(newSpeed)
        if readingMode != .readAloud {
            reinstallTextAdvanceExecutor(
                charactersPerSecond: baseTextCharactersPerSecond * newSpeed
            )
        }
        updateReadingProjection(
            sentence: currentSentenceIndex,
            currentPositionMilliseconds: audioPositionMilliseconds
        )
        updateNowPlayingInfo()
    }

    /// Transfers the visible sentence between the text and audio effect owners.
    /// `pace` is already normalized by Rust Core; Swift only invalidates the old
    /// executor and performs one seek on the new owner.
    func updateReadingPace(_ pace: ReadioReadingPace) {
        let previousMode = readingMode
        let sourceSentence = previousMode == .readAloud
            ? playbackExecutor?.state.currentSentence ?? currentSentenceIndex
            : textAdvanceExecutor?.state.currentSentence ?? currentSentenceIndex
        let sourceWasAdvancing = previousMode == .readAloud
            ? playbackExecutor.map { isActive($0.state.status) } ?? false
            : textAdvanceExecutor?.state.status == .running

        let nextSpeed = pace.audioPlaybackRate
        speed = nextSpeed
        baseTextCharactersPerSecond = pace.textCharactersPerSecond
            / max(nextSpeed, 0.01)
        audioPlayer?.rate = Float(nextSpeed)
        textAdvanceExecutor?.updateCharactersPerSecond(
            pace.textCharactersPerSecond
        )

        if previousMode == pace.mode {
            if pace.mode == .auto {
                runTextExecutor {
                    try $0.seek(
                        sentence: sourceSentence,
                        autoplay: sourceWasAdvancing
                    )
                }
            }
            updateReadingProjection(
                sentence: sourceSentence,
                currentPositionMilliseconds: audioPositionMilliseconds
            )
            updateNowPlayingInfo()
            return
        }

        switch previousMode {
        case .manual:
            break
        case .auto:
            if sourceWasAdvancing {
                runTextExecutor { try $0.pause() }
            }
        case .readAloud:
            if sourceWasAdvancing {
                runExecutor { try $0.pause() }
            }
        }

        readingMode = pace.mode
        switch pace.mode {
        case .manual:
            runTextExecutor {
                try $0.seek(sentence: sourceSentence, autoplay: false)
            }
        case .auto:
            runTextExecutor {
                try $0.seek(sentence: sourceSentence, autoplay: true)
            }
        case .readAloud:
            runExecutor {
                try $0.seek(
                    sentence: sourceSentence,
                    autoplay: sourceWasAdvancing
                )
            }
        }
        updateReadingProjection(sentence: sourceSentence)
        updateNowPlayingInfo()
    }

    func updateVoice(_ newVoice: String?) {
        guard newVoice != voice else { return }
        voice = newVoice
        cache.clear()
        observedAudioDurations.removeAll()
        updateReadingProjection(sentence: currentSentenceIndex)
        cancelAllFetchTasks()
        runExecutor { try $0.audioSourceChanged() }
    }

    func seekToProgress(_ fraction: Double) {
        guard !sentences.isEmpty else { return }
        do {
            let basisPoints = UInt16(
                min(10_000, max(0, (fraction * 10_000).rounded()))
            )
            let cursor = try sdk.resolveReadingSeek(
                segmentCharacterCounts: scalarCharacterCounts,
                progressBasisPoints: basisPoints
            )
            if readingMode != .readAloud {
                currentWordRange = nil
                let autoplay = textAdvanceExecutor?.state.status == .running
                runTextExecutor {
                    try $0.seek(sentence: cursor.segment, autoplay: autoplay)
                }
                updateReadingProjection(
                    sentence: cursor.segment,
                    segmentProgressBasisPoints: cursor.segmentProgressBasisPoints
                )
            } else {
                let autoplay = playbackExecutor.map {
                    isActive($0.state.status)
                } ?? false
                let positionMilliseconds = observedAudioDurations[cursor.segment]
                    .map {
                        $0.saturatingMultiply(
                            by: UInt64(cursor.segmentProgressBasisPoints)
                        ) / 10_000
                    } ?? 0
                runExecutor {
                    try $0.seek(
                        sentence: cursor.segment,
                        positionMilliseconds: positionMilliseconds,
                        autoplay: autoplay
                    )
                }
                updateReadingProjection(
                    sentence: cursor.segment,
                    segmentProgressBasisPoints: cursor.segmentProgressBasisPoints,
                    currentPositionMilliseconds: positionMilliseconds
                )
                updateCurrentHighlight(
                    sentence: cursor.segment,
                    progressBasisPoints: cursor.segmentProgressBasisPoints
                )
            }
        } catch {
            ttsError = "Reading seek unavailable: \(error.localizedDescription)"
        }
    }

    func retry() {
        guard let executor = playbackExecutor,
              executor.state.status == .failed else { return }
        let sentence = executor.state.currentSentence
        cache.remove(sentence)
        fetchTasks.removeValue(forKey: sentence)?.cancel()
        ttsError = nil
        runExecutor { try $0.play() }
    }

    func dismissError() {
        ttsError = nil
    }

    func setSleepDeadline(unixMilliseconds: Int64) {
        runExecutor { try $0.setSleepDeadline(unixMilliseconds: unixMilliseconds) }
    }

    func clearSleepDeadline() {
        runExecutor { try $0.clearSleepDeadline() }
    }

    /// A sentence-count change creates a new portable state value. Reusing the
    /// old reducer state would let completions from the previous document mutate
    /// the new chapter even when their sentence indices happen to match.
    func updateSentences(_ newSentences: [Sentence]) {
        let shouldResume = readingMode == .readAloud
            ? (playbackExecutor.map { isActive($0.state.status) } ?? false)
            : textAdvanceExecutor?.state.status == .running
        // The reader has already installed the new chapter projection when this
        // callback arrives. Invalidating the old reducer must not persist its
        // sentence number against the new DocumentRevision.
        suppressProgressPersistence = true
        pause()
        suppressProgressPersistence = false

        sentences = newSentences
        currentSentenceIndex = 0
        currentWordProgress = 0
        currentWordRange = nil
        progressBasisPoints = 0
        estimatedRemainingSeconds = -1
        ttsError = nil
        observedAudioDurations.removeAll()
        cache.clear()
        cancelAllFetchTasks()
        prefetchTask?.cancel()
        playbackCompletionTask?.cancel()
        tickTask?.cancel()
        textAdvanceTask?.cancel()
        audioPlayer?.stop()
        audioPlayer = nil
        audioPlayerSentence = nil
        playbackExecutor = nil
        installPlaybackExecutor(sentenceCount: newSentences.count)
        textAdvanceExecutor = nil
        installTextAdvanceExecutor()
        if shouldResume && !newSentences.isEmpty {
            play()
        }
    }

    func cleanup() {
        pause()
        isCancelled = true
        isPlaying = false
        isLoading = false
        stopProgressTimer()

        audioPlayer?.stop()
        audioPlayer = nil
        audioPlayerSentence = nil
        playbackCompletionTask?.cancel()
        playbackCompletionTask = nil
        tickTask?.cancel()
        tickTask = nil
        textAdvanceTask?.cancel()
        textAdvanceTask = nil
        prefetchTask?.cancel()
        prefetchTask = nil
        cancelAllFetchTasks()
        cache.clear()
        observedAudioDurations.removeAll()
        ttsProvider.cancelAll()

        teardownAudioSession()
        clearNowPlaying()
        removeRemoteCommands()
    }

    // MARK: - Rust executor lifecycle

    private func installPlaybackExecutor(sentenceCount: Int) {
        do {
            playbackExecutor = try ReadioPlaybackExecutor(
                sentenceCount: sentenceCount,
                platform: self,
                diagnosticProviderID: ttsProvider.diagnosticProviderID
            )
        } catch {
            isPlaying = false
            isLoading = false
            ttsError = "Playback core unavailable: \(error.localizedDescription)"
        }
    }

    private func installTextAdvanceExecutor() {
        do {
            textAdvanceExecutor = try ReadioTextAdvanceExecutor(
                characterCounts: sentences.map { $0.text.unicodeScalars.count },
                charactersPerSecond: baseTextCharactersPerSecond * speed,
                platform: self
            )
        } catch {
            if readingMode != .readAloud {
                ttsError = "Automatic reading unavailable: \(error.localizedDescription)"
            }
        }
    }

    private func reinstallTextAdvanceExecutor(charactersPerSecond: Double) {
        let sentence = textAdvanceExecutor?.state.currentSentence ?? currentSentenceIndex
        let shouldResume = textAdvanceExecutor?.state.status == .running
        textAdvanceTask?.cancel()
        textAdvanceExecutor = nil
        do {
            textAdvanceExecutor = try ReadioTextAdvanceExecutor(
                characterCounts: sentences.map { $0.text.unicodeScalars.count },
                charactersPerSecond: charactersPerSecond,
                platform: self
            )
            try textAdvanceExecutor?.seek(
                sentence: sentence,
                autoplay: shouldResume
            )
        } catch {
            ttsError = "Automatic reading unavailable: \(error.localizedDescription)"
        }
    }

    private func runExecutor(_ command: (ReadioPlaybackExecutor) throws -> Void) {
        guard !isCancelled, let playbackExecutor else { return }
        do {
            try command(playbackExecutor)
        } catch {
            isPlaying = false
            isLoading = false
            ttsError = "Playback state update failed: \(error.localizedDescription)"
        }
    }

    private func runTextExecutor(
        _ command: (ReadioTextAdvanceExecutor) throws -> Void
    ) {
        guard !isCancelled, let textAdvanceExecutor else { return }
        do {
            try command(textAdvanceExecutor)
        } catch {
            isPlaying = false
            ttsError = "Automatic reading failed: \(error.localizedDescription)"
        }
    }

    private func isActive(_ status: ReadioPlaybackStatus) -> Bool {
        status == .loading || status == .playing
    }

    // MARK: - ReadioPlaybackPlatform

    var audioPositionMilliseconds: UInt64 {
        guard let currentTime = audioPlayer?.currentTime,
              currentTime.isFinite,
              currentTime > 0 else { return 0 }
        return UInt64((currentTime * 1_000).rounded())
    }

    func fetchAudio(sentence: Int, generation: UInt64, priority _: String) {
        guard sentences.indices.contains(sentence), !isCancelled else { return }
        Task { @MainActor [weak self] in
            guard let self else { return }
            let result = await self.fetchAudioWithRetry(sentence: sentence)
            guard !self.isCancelled, let executor = self.playbackExecutor else { return }

            do {
                switch result {
                case let .success(_, durationMilliseconds):
                    try executor.audioReady(
                        sentence: sentence,
                        generation: generation,
                        durationMilliseconds: Self.portableMilliseconds(durationMilliseconds)
                    )
                case let .failure(message):
                    try executor.audioFailed(
                        sentence: sentence,
                        generation: generation,
                        message: message
                    )
                }
            } catch {
                self.isPlaying = false
                self.isLoading = false
                self.ttsError = "Playback state update failed: \(error.localizedDescription)"
            }
        }
    }

    func playAudio(
        sentence: Int,
        generation: UInt64,
        offsetMilliseconds: UInt64
    ) {
        guard let cached = cache.get(sentence), !isCancelled else {
            reportPlaybackFailure(
                sentence: sentence,
                generation: generation,
                message: "Audio cache missed for sentence \(sentence + 1), please retry"
            )
            return
        }

        do {
            let player: AVAudioPlayer
            if audioPlayerSentence == sentence, let existing = audioPlayer {
                player = existing
            } else {
                let replacement = try AVAudioPlayer(data: cached.data)
                audioPlayer?.stop()
                audioPlayer = replacement
                audioPlayerSentence = sentence
                player = replacement
            }

            player.enableRate = true
            player.rate = Float(speed)
            player.prepareToPlay()
            player.currentTime = min(
                Double(offsetMilliseconds) / 1_000,
                max(0, player.duration)
            )

            let decodedDuration = player.duration * 1_000
            if decodedDuration.isFinite, decodedDuration > 0 {
                observedAudioDurations[sentence] = Self.portableMilliseconds(
                    decodedDuration
                )
                updateCurrentHighlight(
                    sentence: sentence,
                    progressBasisPoints: Self.highlightBasisPoints(
                        player.currentTime / player.duration
                    )
                )
                updateReadingProjection(
                    sentence: sentence,
                    currentPositionMilliseconds: Self.portableMilliseconds(
                        player.currentTime * 1_000
                    )
                )
            }

            guard player.play() else {
                reportPlaybackFailure(
                    sentence: sentence,
                    generation: generation,
                    message: "Audio playback failed (sentence \(sentence + 1)), please retry"
                )
                return
            }

            startProgressTimer()
            updateNowPlayingInfo()
            observePlaybackCompletion(
                player: player,
                sentence: sentence,
                generation: generation
            )
        } catch {
            reportPlaybackFailure(
                sentence: sentence,
                generation: generation,
                message: "Audio playback failed (sentence \(sentence + 1)), please retry"
            )
        }
    }

    func scheduleTick(generation: UInt64, afterMilliseconds: UInt64) {
        tickTask?.cancel()
        tickTask = Task { @MainActor [weak self] in
            do {
                try await Task.sleep(
                    nanoseconds: afterMilliseconds.saturatingMultiply(by: 1_000_000)
                )
            } catch {
                return
            }
            guard let self, !self.isCancelled else { return }
            let monotonic = Self.portableMilliseconds(
                ProcessInfo.processInfo.systemUptime * 1_000
            )
            let unix = Int64((Date().timeIntervalSince1970 * 1_000).rounded())
            do {
                try self.playbackExecutor?.clockTick(
                    generation: generation,
                    monotonicMilliseconds: monotonic,
                    unixMilliseconds: unix
                )
            } catch {
                self.ttsError = "Playback clock failed: \(error.localizedDescription)"
            }
        }
    }

    func pauseAudio() {
        playbackCompletionTask?.cancel()
        playbackCompletionTask = nil
        tickTask?.cancel()
        tickTask = nil
        audioPlayer?.pause()
        stopProgressTimer()
        updateNowPlayingInfo()
    }

    func persistProgress(sentence: Int, positionMilliseconds _: UInt64) {
        if !suppressProgressPersistence {
            onPersistProgress(sentence)
        }
        updateNowPlayingInfo()
    }

    func prefetchAudio(sentences requestedSentences: [Int]) {
        prefetchTask?.cancel()
        let candidates = requestedSentences.filter {
            sentences.indices.contains($0) && !cache.has($0) && fetchTasks[$0] == nil
        }
        guard !candidates.isEmpty else { return }

        prefetchTask = Task { @MainActor [weak self] in
            guard let self else { return }
            var waveStart = 0
            while waveStart < candidates.count, !Task.isCancelled {
                let waveEnd = min(
                    waveStart + self.maxConcurrentPrefetch,
                    candidates.count
                )
                let wave = Array(candidates[waveStart..<waveEnd])
                await withTaskGroup(of: Void.self) { group in
                    for sentence in wave {
                        group.addTask { [weak self] in
                            _ = await self?.fetchAudioResult(sentence: sentence)
                        }
                    }
                }
                waveStart = waveEnd
            }
        }
    }

    func playbackFailed(sentence _: Int, message: String) {
        ttsError = message
        isPlaying = false
        isLoading = false
        stopProgressTimer()
        updateNowPlayingInfo()
    }

    func playbackCompleted() {
        playbackCompletionTask?.cancel()
        playbackCompletionTask = nil
        tickTask?.cancel()
        tickTask = nil
        stopProgressTimer()
        isPlaying = false
        isLoading = false
        updateNowPlayingInfo()
    }

    func playbackStateChanged(_ state: ReadioPlaybackState) {
        guard readingMode == .readAloud else { return }
        let sentenceChanged = currentSentenceIndex != state.currentSentence
        currentSentenceIndex = state.currentSentence
        isPlaying = state.status == .playing
        isLoading = state.status == .loading
        sleepDeadlineUnixMilliseconds = state.sleepDeadlineUnixMilliseconds
        ttsError = state.error

        if sentenceChanged || state.currentPositionMilliseconds == 0 {
            currentWordProgress = 0
        }
        let highlightProgress: UInt16
        if state.status == .completed {
            highlightProgress = 10_000
        } else if let duration = observedAudioDurations[state.currentSentence],
                  duration > 0 {
            highlightProgress = UInt16(
                min(
                    10_000,
                    state.currentPositionMilliseconds
                        .saturatingMultiply(by: 10_000) / duration
                )
            )
        } else {
            highlightProgress = 0
        }
        updateCurrentHighlight(
            sentence: state.currentSentence,
            progressBasisPoints: highlightProgress
        )
        if !isActive(state.status) {
            stopProgressTimer()
        }
        updateReadingProjection(
            sentence: state.currentSentence,
            segmentProgressBasisPoints: state.status == .completed ? 10_000 : nil,
            currentPositionMilliseconds: state.status == .completed
                ? observedAudioDurations[state.currentSentence]
                    ?? state.currentPositionMilliseconds
                : state.currentPositionMilliseconds
        )
        updateNowPlayingInfo()
    }

    // MARK: - ReadioTextAdvancePlatform

    var monotonicMilliseconds: UInt64 {
        Self.portableMilliseconds(ProcessInfo.processInfo.systemUptime * 1_000)
    }

    func scheduleAdvance(generation: UInt64, afterMilliseconds: UInt64) {
        textAdvanceTask?.cancel()
        textAdvanceTask = Task { @MainActor [weak self] in
            do {
                try await Task.sleep(
                    nanoseconds: afterMilliseconds.saturatingMultiply(by: 1_000_000)
                )
            } catch {
                return
            }
            guard let self, !self.isCancelled else { return }
            self.runTextExecutor { try $0.advanceDue(generation: generation) }
        }
    }

    func cancelScheduledAdvance() {
        textAdvanceTask?.cancel()
        textAdvanceTask = nil
    }

    func persistTextProgress(sentence: Int) {
        if !suppressProgressPersistence {
            onPersistProgress(sentence)
        }
        updateNowPlayingInfo()
    }

    func textAdvanceCompleted() {
        textAdvanceTask?.cancel()
        textAdvanceTask = nil
        isPlaying = false
        updateNowPlayingInfo()
    }

    func textAdvanceStateChanged(_ state: ReadioTextAdvanceState) {
        guard readingMode != .readAloud else { return }
        currentSentenceIndex = state.currentSentence
        currentWordProgress = 0
        currentWordRange = nil
        isPlaying = state.status == .running
        isLoading = false
        if state.status != .completed {
            ttsError = nil
        }
        updateReadingProjection(
            sentence: state.currentSentence,
            segmentProgressBasisPoints: state.status == .completed ? 10_000 : 0
        )
        updateNowPlayingInfo()
    }

    // MARK: - Audio effect helpers

    private func fetchAudioWithRetry(sentence: Int) async -> AudioFetchResult {
        var lastFailure = synthesisErrorMessage(sentenceIndex: sentence, error: nil)
        for attempt in 0...silentRetryCount {
            guard !isCancelled else { return .failure(message: lastFailure) }
            if attempt > 0 {
                do {
                    try await Task.sleep(
                        nanoseconds: silentRetryDelayMilliseconds
                            .saturatingMultiply(by: UInt64(attempt))
                            .saturatingMultiply(by: 1_000_000)
                    )
                } catch {
                    return .failure(message: lastFailure)
                }
            }

            let result = await fetchAudioResult(sentence: sentence)
            switch result {
            case .success:
                return result
            case let .failure(message):
                lastFailure = message
            }
        }
        return .failure(message: lastFailure)
    }

    private func fetchAudioResult(sentence: Int) async -> AudioFetchResult {
        guard !isCancelled, sentences.indices.contains(sentence) else {
            return .failure(message: synthesisErrorMessage(sentenceIndex: sentence, error: nil))
        }
        if let cached = cache.get(sentence) {
            return .success(
                data: cached.data,
                durationMilliseconds: cached.durationMs
            )
        }
        if let existing = fetchTasks[sentence] {
            return await existing.value
        }

        let task = Task<AudioFetchResult, Never> { @MainActor [weak self] in
            guard let self else {
                return .failure(message: "TTS synthesis was cancelled")
            }
            return await self.renderAudio(sentence: sentence)
        }
        fetchTasks[sentence] = task
        let result = await task.value
        fetchTasks.removeValue(forKey: sentence)
        return result
    }

    private func renderAudio(sentence index: Int) async -> AudioFetchResult {
        guard sentences.indices.contains(index) else {
            return .failure(message: synthesisErrorMessage(sentenceIndex: index, error: nil))
        }
        let text = sentences[index].text
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return .failure(message: synthesisErrorMessage(sentenceIndex: index, error: nil))
        }

        do {
            let audio = try await ttsProvider.render(
                ReadioNativeSpeechRequest(
                    text: text,
                    voiceID: voice,
                    language: nil,
                    rateMultiplier: 1,
                    pitchMultiplier: 1,
                    volume: 1
                )
            )
            cache.set(
                index,
                data: audio.data,
                durationMs: audio.durationMilliseconds
            )
            return .success(
                data: audio.data,
                durationMilliseconds: audio.durationMilliseconds
            )
        } catch {
            return .failure(
                message: synthesisErrorMessage(sentenceIndex: index, error: error)
            )
        }
    }

    private func synthesisErrorMessage(sentenceIndex: Int, error: Error?) -> String {
        if let presentable = error as? any ReadioTTSUserPresentableError {
            return presentable.ttsUserMessage
        }
        let reason = error.map { String(describing: $0) } ?? "the provider returned no audio"
        return "\(ttsProvider.displayName) failed for sentence \(sentenceIndex + 1): \(reason). Retry or choose another voice."
    }

    private func reportPlaybackFailure(
        sentence: Int,
        generation: UInt64,
        message: String
    ) {
        do {
            try playbackExecutor?.audioFailed(
                sentence: sentence,
                generation: generation,
                message: message
            )
        } catch {
            ttsError = message
            isPlaying = false
            isLoading = false
        }
    }

    private func observePlaybackCompletion(
        player: AVAudioPlayer,
        sentence: Int,
        generation: UInt64
    ) {
        playbackCompletionTask?.cancel()
        playbackCompletionTask = Task { @MainActor [weak self, weak player] in
            guard let self, let player else { return }
            while player.isPlaying, !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 50_000_000)
            }
            guard !Task.isCancelled,
                  !self.isCancelled,
                  self.audioPlayer === player else { return }
            do {
                try self.playbackExecutor?.audioEnded(
                    sentence: sentence,
                    generation: generation
                )
            } catch {
                self.ttsError = "Playback completion failed: \(error.localizedDescription)"
            }
        }
    }

    private func cancelAllFetchTasks() {
        for task in fetchTasks.values {
            task.cancel()
        }
        fetchTasks.removeAll()
    }

    private static func portableMilliseconds(_ value: Double) -> UInt64 {
        guard value.isFinite, value > 0 else { return 0 }
        return UInt64(value.rounded())
    }

    // MARK: - Progress projection

    private func startProgressTimer() {
        stopProgressTimer()
        progressTimer = Timer.scheduledTimer(
            withTimeInterval: 0.1,
            repeats: true
        ) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self,
                      let player = self.audioPlayer,
                      player.duration > 0,
                      self.sentences.indices.contains(self.currentSentenceIndex) else { return }
                let progress = player.currentTime / player.duration
                self.currentWordProgress = progress
                self.updateCurrentHighlight(
                    sentence: self.currentSentenceIndex,
                    progressBasisPoints: Self.highlightBasisPoints(progress)
                )
                self.updateReadingProjection(
                    sentence: self.currentSentenceIndex,
                    currentPositionMilliseconds: Self.portableMilliseconds(
                        player.currentTime * 1_000
                    )
                )
            }
        }
    }

    private func stopProgressTimer() {
        progressTimer?.invalidate()
        progressTimer = nil
    }

    private var scalarCharacterCounts: [UInt64] {
        sentences.map { UInt64($0.text.unicodeScalars.count) }
    }

    private func updateCurrentHighlight(
        sentence: Int,
        progressBasisPoints: UInt16
    ) {
        guard readingMode == .readAloud,
              let highlightDocument,
              sentences.indices.contains(sentence),
              let sentenceIndex = UInt32(
                  exactly: sentences[sentence].readerIndex
              ) else {
            currentWordRange = nil
            return
        }
        do {
            let target = try sdk.resolveHighlight(
                document: highlightDocument,
                cursor: ReadioHighlightCursor(
                    documentRevision: highlightDocument.documentRevision,
                    sentenceIndex: sentenceIndex,
                    wordIndex: nil,
                    wordProgressBasisPoints: progressBasisPoints
                )
            )
            currentWordRange = target.sentenceIndex == sentenceIndex
                ? target.sentenceLocalWordRange
                : nil
        } catch {
            currentWordRange = nil
        }
    }

    private static func highlightBasisPoints(_ fraction: Double) -> UInt16 {
        guard fraction.isFinite else { return 0 }
        return UInt16(min(10_000, max(0, (fraction * 10_000).rounded())))
    }

    private func updateReadingProjection(
        sentence: Int,
        segmentProgressBasisPoints: UInt16? = nil,
        currentPositionMilliseconds: UInt64 = 0
    ) {
        let currentSegment = sentences.isEmpty
            ? 0
            : min(max(0, sentence), sentences.count - 1)
        let observedDuration = observedAudioDurations[currentSegment]
        let derivedSegmentProgress: UInt16
        if let observedDuration, observedDuration > 0 {
            derivedSegmentProgress = UInt16(
                min(
                    10_000,
                    currentPositionMilliseconds
                        .saturatingMultiply(by: 10_000) / observedDuration
                )
            )
        } else {
            derivedSegmentProgress = 0
        }
        let timing: ReadioReadingTimingEvidence
        switch readingMode {
        case .manual:
            timing = .unknown
        case .auto:
            timing = .textRate(
                charactersPerSecond: baseTextCharactersPerSecond * speed
            )
        case .readAloud:
            let observations = observedAudioDurations
                .map {
                    ReadioAudioDurationObservation(
                        segment: $0.key,
                        durationMilliseconds: $0.value
                    )
                }
                .sorted { $0.segment < $1.segment }
            timing = observations.isEmpty
                ? .unknown
                : .observedAudio(
                    playbackRate: speed,
                    currentPositionMilliseconds: currentPositionMilliseconds,
                    durations: observations
                )
        }

        do {
            let projection = try sdk.projectReadingProgress(
                ReadioReadingProgressRequest(
                    segmentCharacterCounts: scalarCharacterCounts,
                    currentSegment: currentSegment,
                    currentSegmentProgressBasisPoints:
                        segmentProgressBasisPoints ?? derivedSegmentProgress,
                    timing: timing
                )
            )
            progressBasisPoints = projection.progressBasisPoints
            estimatedRemainingSeconds = projection.remainingTime.seconds
                .map { Double($0) } ?? -1
        } catch {
            estimatedRemainingSeconds = -1
            ttsError = "Reading progress unavailable: \(error.localizedDescription)"
        }
    }

    // MARK: - Audio session

    /// Platform interruption facts still enter the portable reducer through its
    /// normal pause/play events, preserving generation and sampled cursor rules.
    func audioInterruptionBegan() {
        guard readingMode == .readAloud,
              let executor = playbackExecutor else { return }
        shouldResumeAfterAudioInterruption = isActive(executor.state.status)
        if shouldResumeAfterAudioInterruption {
            pause()
        }
    }

    func audioInterruptionEnded(shouldResume: Bool) {
        let resume = shouldResumeAfterAudioInterruption && shouldResume
        shouldResumeAfterAudioInterruption = false
        guard resume, !isCancelled else { return }
        #if os(iOS)
        try? AVAudioSession.sharedInstance().setActive(true)
        #endif
        play()
    }

    func audioRouteBecameUnavailable() {
        shouldResumeAfterAudioInterruption = false
        guard readingMode == .readAloud,
              let executor = playbackExecutor,
              isActive(executor.state.status) else { return }
        pause()
    }

    private func setupAudioSession() {
        #if os(iOS)
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playback, mode: .spokenAudio, options: [])
            try session.setActive(true)
            observeAudioSession(session)
        } catch {
            print("[TTSPlaybackEngine] Failed to configure audio session: \(error)")
        }
        #endif
    }

    private func teardownAudioSession() {
        #if os(iOS)
        let notifications = NotificationCenter.default
        for observer in audioSessionObservers {
            notifications.removeObserver(observer)
        }
        audioSessionObservers.removeAll()
        do {
            try AVAudioSession.sharedInstance().setActive(
                false,
                options: .notifyOthersOnDeactivation
            )
        } catch {
            print("[TTSPlaybackEngine] Failed to deactivate audio session: \(error)")
        }
        #endif
    }

    #if os(iOS)
    private func observeAudioSession(_ session: AVAudioSession) {
        guard audioSessionObservers.isEmpty else { return }
        let notifications = NotificationCenter.default
        audioSessionObservers.append(
            notifications.addObserver(
                forName: AVAudioSession.interruptionNotification,
                object: session,
                queue: .main
            ) { [weak self] notification in
                guard let rawType = notification.userInfo?[AVAudioSessionInterruptionTypeKey]
                    as? UInt,
                      let type = AVAudioSession.InterruptionType(rawValue: rawType) else { return }
                let rawOptions = notification.userInfo?[AVAudioSessionInterruptionOptionKey]
                    as? UInt ?? 0
                let shouldResume = AVAudioSession.InterruptionOptions(rawValue: rawOptions)
                    .contains(.shouldResume)
                Task { @MainActor [weak self] in
                    switch type {
                    case .began:
                        self?.audioInterruptionBegan()
                    case .ended:
                        self?.audioInterruptionEnded(shouldResume: shouldResume)
                    @unknown default:
                        self?.audioInterruptionBegan()
                    }
                }
            }
        )
        audioSessionObservers.append(
            notifications.addObserver(
                forName: AVAudioSession.routeChangeNotification,
                object: session,
                queue: .main
            ) { [weak self] notification in
                guard let rawReason = notification.userInfo?[AVAudioSessionRouteChangeReasonKey]
                    as? UInt,
                      let reason = AVAudioSession.RouteChangeReason(rawValue: rawReason),
                      reason == .oldDeviceUnavailable else { return }
                Task { @MainActor [weak self] in
                    self?.audioRouteBecameUnavailable()
                }
            }
        )
    }
    #endif

    // MARK: - Now Playing

    private func updateNowPlayingInfo() {
        var info = [String: Any]()
        info[MPMediaItemPropertyTitle] = itemTitle
        info[MPMediaItemPropertyArtist] = "Readio"
        if let player = audioPlayer {
            info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = player.currentTime
            info[MPMediaItemPropertyPlaybackDuration] = player.duration
            info[MPNowPlayingInfoPropertyPlaybackRate] = isPlaying ? speed : 0
        }
        info[MPMediaItemPropertyAlbumTitle] =
            "Sentence \(currentSentenceIndex + 1) / \(sentences.count)"
        MPNowPlayingInfoCenter.default().nowPlayingInfo = info
    }

    private func clearNowPlaying() {
        MPNowPlayingInfoCenter.default().nowPlayingInfo = nil
    }

    // MARK: - Remote commands

    private func setupRemoteCommands() {
        let center = MPRemoteCommandCenter.shared()
        center.playCommand.isEnabled = true
        center.playCommand.addTarget { [weak self] _ in
            Task { @MainActor in self?.play() }
            return .success
        }
        center.pauseCommand.isEnabled = true
        center.pauseCommand.addTarget { [weak self] _ in
            Task { @MainActor in self?.pause() }
            return .success
        }
        center.togglePlayPauseCommand.isEnabled = true
        center.togglePlayPauseCommand.addTarget { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                self.isPlaying ? self.pause() : self.play()
            }
            return .success
        }
        center.nextTrackCommand.isEnabled = true
        center.nextTrackCommand.addTarget { [weak self] _ in
            Task { @MainActor in self?.nextSentence() }
            return .success
        }
        center.previousTrackCommand.isEnabled = true
        center.previousTrackCommand.addTarget { [weak self] _ in
            Task { @MainActor in self?.prevSentence() }
            return .success
        }
    }

    private func removeRemoteCommands() {
        let center = MPRemoteCommandCenter.shared()
        center.playCommand.removeTarget(nil)
        center.pauseCommand.removeTarget(nil)
        center.togglePlayPauseCommand.removeTarget(nil)
        center.nextTrackCommand.removeTarget(nil)
        center.previousTrackCommand.removeTarget(nil)
    }
}

private extension UInt64 {
    func saturatingMultiply(by other: UInt64) -> UInt64 {
        let (result, overflow) = multipliedReportingOverflow(by: other)
        return overflow ? .max : result
    }
}
