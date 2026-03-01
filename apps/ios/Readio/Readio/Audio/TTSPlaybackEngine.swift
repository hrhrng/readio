import Foundation
import AVFoundation
import MediaPlayer

// MARK: - TTSPlaybackEngine
/// Core TTS playback engine that orchestrates audio synthesis, caching, and playback.
///
/// This is a faithful port of the web app's `use-tts-player.ts` React hook, adapted
/// to Swift's @Observable + async/await + AVAudioPlayer paradigm.
///
/// ## Architecture
///
/// The engine manages a pipeline with four stages:
///
/// 1. **Job creation:** `POST /api/tts/jobs` enqueues synthesis on the server.
/// 2. **Polling:** `GET /api/tts/jobs/{id}` until the job reaches a terminal state.
/// 3. **Decoding:** Base64-encoded audio data is decoded into raw `Data` bytes.
/// 4. **Playback:** `AVAudioPlayer(data:)` plays the decoded audio with background
///    audio session support and lock screen controls.
///
/// ## Key behaviors (matching the web app)
///
/// - **Prefetch:** After each sentence index change, the next 5 sentences are
///   fetched in waves of 2 concurrent requests. This saturates the backend's
///   worker pool without creating a long queue that might time out.
///
/// - **LRU cache:** 50-entry `AudioCache` avoids re-synthesis when the user
///   seeks back to a previously played sentence.
///
/// - **Silent retry:** Up to 2 automatic retries with linear backoff before
///   surfacing an error to the user. Permanent backend failures (quota exceeded,
///   invalid text) skip retries entirely.
///
/// - **Dedup:** In-flight fetch tasks are tracked by sentence index. Concurrent
///   requests for the same sentence reuse the existing task instead of creating
///   duplicate TTS jobs.
///
/// - **Session management:** Each engine instance creates a unique session ID.
///   On cleanup, `POST /api/tts/sessions/{id}/cancel` bulk-cancels all pending
///   jobs for that session.
///
/// ## iOS-specific additions
///
/// - `AVAudioSession` configured for `.playback` + `.spokenAudio` to support
///   background audio and proper ducking behavior.
/// - `MPNowPlayingInfoCenter` + `MPRemoteCommandCenter` for lock screen and
///   Control Center integration.
/// - Word-level progress tracked via a `Timer` (~10 Hz) instead of the web's
///   `ontimeupdate` event.
///
/// ## Threading
///
/// All published state is annotated with `@MainActor` to ensure SwiftUI views
/// observe changes on the main thread. Internal async work runs on cooperative
/// Swift concurrency pools.
@MainActor
@Observable
final class TTSPlaybackEngine {

    // MARK: - Published State (observed by views)

    /// Whether audio is currently playing. Setting this to `true` triggers the
    /// playback loop; setting it to `false` pauses the current audio.
    var isPlaying = false

    /// Whether the engine is waiting for audio data (first fetch, before silent
    /// retries). The UI shows a loading spinner during this phase.
    var isLoading = false

    /// Zero-based index of the sentence currently being played or about to play.
    var currentSentenceIndex = 0

    /// Fractional word-level progress within the current sentence (0.0 – 1.0).
    /// Used for per-word highlighting: `floor(progress * tokenCount)` gives the
    /// index of the currently highlighted token.
    var currentWordProgress: Double = 0

    /// Estimated seconds remaining until the end of the document, based on a
    /// rolling average of recent sentence durations. Negative means "unknown".
    var estimatedRemainingSeconds: Double = -1

    /// Non-nil when the current sentence failed after all retries. The UI should
    /// display this message with a "Retry" button. Cleared on `retry()` or
    /// `dismissError()`.
    var ttsError: String? = nil

    // MARK: - Configuration (set at init, updated via public methods)

    /// The library item ID this engine is synthesizing audio for.
    private(set) var itemId: String

    /// The full sentence list for the current chapter/document.
    private(set) var sentences: [Sentence]

    /// TTS playback speed multiplier (e.g., 1.0 = normal, 1.5 = 50% faster).
    /// Changing this clears the cache and restarts the current sentence if playing.
    private var speed: Double

    /// TTS voice identifier override. When nil, the server uses its default voice.
    /// Changing this clears the cache and restarts the current sentence if playing.
    private var voice: String?

    /// Human-readable title for MPNowPlayingInfoCenter display.
    private var itemTitle: String

    // MARK: - Internal State

    /// The AVAudioPlayer instance for the currently playing sentence.
    /// Replaced each time a new sentence starts playing.
    private var audioPlayer: AVAudioPlayer?

    /// LRU cache mapping sentence index → decoded audio data + duration.
    /// Shared between the playback loop and prefetch logic.
    private let cache = AudioCache(capacity: 50)

    /// Service layer for creating and polling TTS jobs on the backend.
    private let ttsService = TTSService()

    /// Unique session identifier for this engine instance. Format:
    /// `session-{itemId}-{timestamp}-{random6chars}`, matching the web app.
    /// Used for bulk job cancellation on cleanup.
    private var sessionId: String

    /// Rolling list of actual audio durations (ms) for recent sentences.
    /// Used to compute `estimatedRemainingSeconds` via a sliding-window average.
    private var durations: [Double] = []

    /// Set to `true` during `cleanup()` to signal all in-flight tasks to bail out.
    private var isCancelled = false

    /// Set of TTS job IDs currently being polled. Used for diagnostics; not
    /// strictly necessary for dedup (that's handled by `fetchTasks`).
    private var pendingJobs = Set<String>()

    /// In-flight fetch dedup map: sentence index → the running Task that will
    /// produce audio data. If a second request arrives for the same sentence,
    /// it awaits the existing task instead of creating a duplicate TTS job.
    private var fetchTasks = [Int: Task<(Data, Double)?, Never>]()

    /// The main playback loop task. Cancelled and recreated each time playback
    /// state changes (play/pause/seek).
    private var playbackTask: Task<Void, Never>?

    /// The prefetch task that eagerly fetches upcoming sentences. Cancelled and
    /// recreated each time the sentence index changes.
    private var prefetchTask: Task<Void, Never>?

    /// Timer that fires ~10x/sec during playback to update `currentWordProgress`
    /// and `estimatedRemainingSeconds` for smooth UI animation.
    private var progressTimer: Timer?

    /// Guard flag to prevent concurrent retry loops from rapid "Retry" button taps.
    private var isRetrying = false

    // MARK: - Constants (matching the web app's tuning)

    /// Maximum time (ms) to poll a single TTS job before giving up.
    private let maxPollMs: Double = 30_000

    /// Number of silent auto-retries before surfacing an error to the user.
    /// These run without a loading spinner or error UI.
    private let silentRetryCount = 2

    /// Base delay (ms) between silent retries; actual delay = base * attempt number.
    private let silentRetryDelayMs: Double = 800

    /// Number of sentences to prefetch ahead of the current playback position.
    /// Kept small to match the backend's 2-worker limit (~5s per job).
    private let prefetchWindow = 5

    /// Maximum concurrent prefetch requests per wave. Matches the backend worker
    /// count so we saturate capacity without piling up a long timeout-prone queue.
    private let maxConcurrentPrefetch = 2

    // MARK: - Initialization

    /// Create a new TTS playback engine for the given item.
    ///
    /// - Parameters:
    ///   - itemId: Library item ID (used in TTS job requests and session naming).
    ///   - sentences: The sentence list to play through sequentially.
    ///   - speed: Initial playback speed multiplier.
    ///   - voice: Initial voice ID override (nil = server default).
    ///   - itemTitle: Display title for lock screen / Now Playing info.
    init(itemId: String, sentences: [Sentence], speed: Double, voice: String?, itemTitle: String) {
        self.itemId = itemId
        self.sentences = sentences
        self.speed = speed
        self.voice = voice
        self.itemTitle = itemTitle

        // Generate a unique session ID matching the web app's format:
        // "session-{itemId}-{timestamp}-{random6chars}"
        let timestamp = Int(Date().timeIntervalSince1970 * 1000)
        let randomSuffix = String((0..<6).map { _ in
            "abcdefghijklmnopqrstuvwxyz0123456789".randomElement()!
        })
        self.sessionId = "session-\(itemId)-\(timestamp)-\(randomSuffix)"

        setupAudioSession()
        setupRemoteCommands()
    }

    // MARK: - Public API

    /// Start or resume playback from the current sentence index.
    /// If already playing, this is a no-op.
    func play() {
        guard !isPlaying else { return }
        isPlaying = true
        startPlayback()
    }

    /// Pause playback immediately.
    /// The audio player is paused (not destroyed) so `play()` can resume.
    func pause() {
        isPlaying = false
        audioPlayer?.pause()
        stopProgressTimer()
        updateNowPlayingInfo()
    }

    /// Jump to a specific sentence and begin playback.
    ///
    /// Equivalent to `seekToSentence(index)` + `play()`, but batched so the
    /// playback loop only triggers once with the new position.
    ///
    /// - Parameter index: Zero-based sentence index to start playing from.
    func playFromSentence(_ index: Int) {
        guard index >= 0 && index < sentences.count else { return }
        currentSentenceIndex = index
        currentWordProgress = 0
        isPlaying = true
        startPlayback()
    }

    /// Move the playback cursor to a sentence without triggering playback.
    ///
    /// Used for restoring reading position on page load — the user sees the
    /// correct sentence highlighted but audio doesn't start automatically.
    ///
    /// - Parameter index: Zero-based sentence index to seek to.
    func seekToSentence(_ index: Int) {
        guard index >= 0 && index < sentences.count else { return }
        currentSentenceIndex = index
        currentWordProgress = 0
    }

    /// Advance to the next sentence. If already at the last sentence, this is a no-op.
    /// If currently playing, playback continues from the new position.
    func nextSentence() {
        let next = currentSentenceIndex + 1
        guard next < sentences.count else { return }
        currentSentenceIndex = next
        currentWordProgress = 0
        if isPlaying {
            startPlayback()
        }
        prefetchAhead()
    }

    /// Go to the previous sentence, or restart the current sentence if more than
    /// 2 seconds have elapsed (matching standard media player behavior).
    func prevSentence() {
        // If we're more than 2 seconds into the current audio, restart it
        // instead of going back (same UX as the web app and music players)
        if let player = audioPlayer, player.currentTime > 2 {
            player.currentTime = 0
            currentWordProgress = 0
            return
        }
        let prev = max(0, currentSentenceIndex - 1)
        currentSentenceIndex = prev
        currentWordProgress = 0
        if isPlaying {
            startPlayback()
        }
    }

    /// Update the playback speed. If currently playing, the current sentence is
    /// restarted with the new speed (since TTS audio is speed-specific).
    ///
    /// - Parameter newSpeed: The new speed multiplier (e.g., 1.0, 1.5, 2.0).
    func updateSpeed(_ newSpeed: Double) {
        guard newSpeed != speed else { return }
        speed = newSpeed

        // Audio is generated at a specific speed, so cached entries are stale
        cache.clear()
        cancelAllFetchTasks()

        // If playing, restart the current sentence with the new speed
        if isPlaying {
            startPlayback()
        }
    }

    /// Update the TTS voice. If currently playing, the current sentence is
    /// restarted with the new voice (since audio is voice-specific).
    ///
    /// - Parameter newVoice: The new voice ID, or nil to use the server default.
    func updateVoice(_ newVoice: String?) {
        guard newVoice != voice else { return }
        voice = newVoice

        // Audio is generated with a specific voice, so cached entries are stale
        cache.clear()
        cancelAllFetchTasks()

        // If playing, restart the current sentence with the new voice
        if isPlaying {
            startPlayback()
        }
    }

    /// Seek to a fractional position within the document.
    ///
    /// Maps a 0.0–1.0 fraction to a sentence index: `round(fraction * (count - 1))`.
    /// If currently playing, playback continues from the new position.
    ///
    /// - Parameter fraction: Position as a fraction of total sentences (0.0 = start, 1.0 = end).
    func seekToProgress(_ fraction: Double) {
        let len = sentences.count
        guard len > 0 else { return }
        let targetIdx = min(max(0, Int(round(fraction * Double(len - 1)))), len - 1)
        currentSentenceIndex = targetIdx
        currentWordProgress = 0
        if isPlaying {
            startPlayback()
        }
    }

    /// Retry the current sentence after a TTS error.
    ///
    /// Clears the error, removes stale cache/dedup entries, and attempts to
    /// re-fetch the audio. On success, playback resumes automatically.
    /// Debounced via `isRetrying` to prevent concurrent retry loops from rapid taps.
    func retry() {
        guard !isRetrying else { return }
        isRetrying = true

        let idx = currentSentenceIndex
        cache.remove(idx)
        fetchTasks[idx]?.cancel()
        fetchTasks.removeValue(forKey: idx)
        ttsError = nil

        // Clear prefetch dedup entries to free backend workers for the current sentence
        for key in fetchTasks.keys where key != idx {
            fetchTasks[key]?.cancel()
            fetchTasks.removeValue(forKey: key)
        }

        Task { [weak self] in
            guard let self else { return }
            defer { Task { @MainActor in self.isRetrying = false } }

            let maxRetries = 3
            for attempt in 0..<maxRetries {
                if self.isCancelled { return }

                // Linear backoff between retries (skip delay on first attempt)
                if attempt > 0 {
                    try? await Task.sleep(nanoseconds: UInt64(self.silentRetryDelayMs * Double(attempt)) * 1_000_000)
                }
                if self.isCancelled { return }

                let result = await self.fetchAudioOnce(sentenceIdx: idx, priority: .user)

                // Permanent backend failure — surface specific error, no more retries
                if let (_, _) = result {
                    // Audio now in cache — trigger playback (cache hit = instant, no loading)
                    await MainActor.run {
                        self.isPlaying = true
                        self.startPlayback()
                    }
                    return
                }
            }

            // All retries failed — re-show error
            await MainActor.run {
                self.ttsError = "TTS synthesis failed (sentence \(idx + 1)), please retry"
            }
        }
    }

    /// Dismiss the current error without retrying. The engine stays paused
    /// on the same sentence so the user can manually seek elsewhere.
    func dismissError() {
        ttsError = nil
    }

    /// Replace the sentence list (e.g., when switching EPUB chapters).
    /// Resets the playback position to the beginning and clears caches.
    ///
    /// - Parameter newSentences: The new sentence array to use.
    func updateSentences(_ newSentences: [Sentence]) {
        let wasPlaying = isPlaying
        pause()

        sentences = newSentences
        currentSentenceIndex = 0
        currentWordProgress = 0
        estimatedRemainingSeconds = -1
        ttsError = nil
        durations.removeAll()
        cache.clear()
        cancelAllFetchTasks()

        if wasPlaying && !newSentences.isEmpty {
            play()
        }
    }

    /// Tear down the engine: cancel all in-flight work, release the audio session,
    /// and remove remote command handlers.
    ///
    /// Call this when the player screen is dismissed or the item changes.
    func cleanup() {
        isCancelled = true
        isPlaying = false
        stopProgressTimer()

        // Stop and release the audio player
        audioPlayer?.stop()
        audioPlayer = nil

        // Cancel all in-flight tasks
        playbackTask?.cancel()
        playbackTask = nil
        prefetchTask?.cancel()
        prefetchTask = nil
        cancelAllFetchTasks()

        // Release caches
        cache.clear()
        durations.removeAll()

        // Cancel pending backend jobs for this session
        Task { [sessionId, ttsService] in
            try? await ttsService.cancelSession(id: sessionId)
        }

        // Deactivate audio session and clear now-playing info
        teardownAudioSession()
        clearNowPlaying()
        removeRemoteCommands()
    }

    // MARK: - Core Playback Loop

    /// The main playback engine. Cancels any existing playback task and starts
    /// a new one that fetches audio for the current sentence, plays it, and
    /// auto-advances to the next sentence on completion.
    ///
    /// This mirrors the web app's "Playback Effect" — the single loop that
    /// drives all audio playback, triggered by changes to sentence index or
    /// play/pause state.
    private func startPlayback() {
        // Cancel the previous playback task (equivalent to the web effect's cleanup)
        playbackTask?.cancel()
        stopProgressTimer()

        let idx = currentSentenceIndex
        guard idx >= 0 && idx < sentences.count && isPlaying else {
            isPlaying = false
            return
        }

        playbackTask = Task { [weak self] in
            guard let self, !Task.isCancelled else { return }

            await MainActor.run {
                self.currentWordProgress = 0
                self.isLoading = true
                self.ttsError = nil
            }

            // Phase 1: Check cache → reuse in-flight prefetch → first attempt (visible loading)
            var audioResult: (Data, Double)?

            // Check cache first (instant return if cached)
            if let cached = self.cache.get(idx) {
                audioResult = cached
            } else {
                // Reuse an in-flight prefetch task if one exists for this sentence,
                // otherwise make a fresh request
                if let existingTask = self.fetchTasks[idx] {
                    audioResult = await existingTask.value
                } else {
                    audioResult = await self.fetchAudioOnce(sentenceIdx: idx, priority: .user)
                }
            }

            guard !Task.isCancelled else { return }
            await MainActor.run { self.isLoading = false }

            // Phase 2: Silent auto-retries (no loading spinner, no error UI)
            // Up to silentRetryCount additional attempts with linear backoff.
            if audioResult == nil {
                for attempt in 1...self.silentRetryCount {
                    if Task.isCancelled { return }
                    try? await Task.sleep(nanoseconds: UInt64(self.silentRetryDelayMs * Double(attempt)) * 1_000_000)
                    if Task.isCancelled { return }

                    audioResult = await self.fetchAudioOnce(sentenceIdx: idx, priority: .user)
                    if audioResult != nil { break }
                }
            }

            guard !Task.isCancelled else { return }

            // All retries exhausted — pause and surface the error
            guard let (audioData, durationMs) = audioResult else {
                await MainActor.run {
                    self.ttsError = "TTS synthesis failed (sentence \(idx + 1)), please retry"
                    self.isPlaying = false
                }
                return
            }

            // Record the duration for remaining-time estimation
            await MainActor.run {
                self.durations.append(durationMs)
                self.updateEstimate(idx: idx)
            }

            // Create and configure the AVAudioPlayer for this sentence
            do {
                let player = try AVAudioPlayer(data: audioData)
                player.enableRate = true
                player.rate = Float(self.speed)
                player.prepareToPlay()

                guard !Task.isCancelled else { return }

                await MainActor.run {
                    // Tear down the previous player cleanly
                    self.audioPlayer?.stop()
                    self.audioPlayer = player

                    // Override backend duration with the actual decoded duration if available
                    let realDurationMs = player.duration * 1000
                    if realDurationMs > 0 && realDurationMs.isFinite {
                        if !self.durations.isEmpty {
                            self.durations[self.durations.count - 1] = realDurationMs
                        }
                        self.updateEstimate(idx: idx)
                    }

                    // Start the progress timer for word-level highlighting
                    self.startProgressTimer()

                    // Update lock screen info
                    self.updateNowPlayingInfo()
                }

                // Start playback
                player.play()

                // Wait for playback to finish by polling (AVAudioPlayer doesn't have
                // async completion; the delegate pattern requires a separate class).
                // Poll at ~20 Hz which is lightweight and gives responsive completion detection.
                while player.isPlaying && !Task.isCancelled {
                    try? await Task.sleep(nanoseconds: 50_000_000) // 50ms
                }

                guard !Task.isCancelled else { return }

                // Auto-advance to the next sentence
                await MainActor.run {
                    self.stopProgressTimer()
                    if idx + 1 < self.sentences.count {
                        self.currentSentenceIndex = idx + 1
                        self.prefetchAhead()
                        self.startPlayback()
                    } else {
                        // Reached the end of the document
                        self.isPlaying = false
                        self.updateNowPlayingInfo()
                    }
                }
            } catch {
                guard !Task.isCancelled else { return }
                await MainActor.run {
                    self.ttsError = "Audio playback failed (sentence \(idx + 1)), please retry"
                    self.isPlaying = false
                }
            }
        }

        // Kick off prefetch for upcoming sentences
        prefetchAhead()
    }

    // MARK: - Audio Fetching

    /// Single-attempt TTS fetch: create a job, poll until done, decode audio.
    ///
    /// Returns `(Data, Double)` on success (decoded audio bytes + duration in ms),
    /// or `nil` on failure/cancellation.
    ///
    /// This is the atomic building block — retry logic is managed by callers
    /// (the playback loop for silent retries, `retry()` for user-initiated retries).
    ///
    /// - Parameters:
    ///   - sentenceIdx: Index of the sentence to synthesize.
    ///   - priority: Job priority (`.user` for current sentence, `.prefetch` for look-ahead).
    /// - Returns: Tuple of (audio data, duration in ms) on success, nil on failure.
    @MainActor
    private func fetchAudioOnce(sentenceIdx: Int, priority: TTSJobPriority) async -> (Data, Double)? {
        guard sentenceIdx < sentences.count else { return nil }
        let sentence = sentences[sentenceIdx]
        guard !sentence.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }

        do {
            // Create the TTS job on the backend
            let params = TTSJobCreateParams(
                sessionId: sessionId,
                itemId: itemId,
                chapterId: "main",
                priority: priority,
                provider: nil,
                request: TTSRequest(
                    text: sentence.text,
                    speed: speed,
                    voice: voice
                )
            )

            var status = try await ttsService.createJob(params: params)
            let jobId = status.jobId
            pendingJobs.insert(jobId)

            // Poll until the job reaches a terminal state, with exponential backoff
            // and a hard timeout to prevent infinite loops on stuck jobs.
            var delay: UInt64 = 200_000_000 // 200ms in nanoseconds
            var elapsedMs: Double = 0

            while status.status == .queued || status.status == .running {
                if isCancelled || Task.isCancelled {
                    pendingJobs.remove(jobId)
                    return nil
                }
                if elapsedMs >= maxPollMs {
                    // Job timed out — treat as a transient failure (caller may retry)
                    pendingJobs.remove(jobId)
                    return nil
                }

                try? await Task.sleep(nanoseconds: delay)
                let delayMs = Double(delay) / 1_000_000
                elapsedMs += delayMs
                delay = min(UInt64(Double(delay) * 1.5), 2_000_000_000) // cap at 2s

                status = try await ttsService.pollJob(id: jobId, includeAudio: true)
            }

            pendingJobs.remove(jobId)

            // Permanent backend failure — return nil (caller decides whether to retry)
            if status.status == .failed {
                return nil
            }

            // If completed but audio wasn't included (cache hit on create), fetch it explicitly
            if status.status == .completed && status.audioBase64 == nil {
                status = try await ttsService.pollJob(id: jobId, includeAudio: true)
            }

            // Decode the base64 audio and cache it
            if status.status == .completed, let base64String = status.audioBase64,
               let audioData = Data(base64Encoded: base64String) {
                let durationMs = status.durationMs ?? 3000 // fallback if backend doesn't report
                cache.set(sentenceIdx, data: audioData, durationMs: durationMs)
                return (audioData, durationMs)
            }

            return nil
        } catch {
            // Network error, decoding error, etc. — treat as transient failure
            return nil
        }
    }

    /// Ref-stable audio fetcher shared by the playback loop and prefetch logic.
    ///
    /// Returns cached audio instantly, or deduplicates in-flight requests by
    /// reusing existing tasks for the same sentence index.
    ///
    /// - Parameters:
    ///   - sentenceIdx: Index of the sentence to fetch.
    ///   - priority: Job priority level.
    /// - Returns: Tuple of (audio data, duration in ms) on success, nil on failure.
    private func fetchAudio(sentenceIdx: Int, priority: TTSJobPriority) async -> (Data, Double)? {
        if isCancelled { return nil }

        let sentence = sentences[safe: sentenceIdx]
        guard let sentence, !sentence.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return nil
        }

        // 1) Already cached — instant return
        if let cached = cache.get(sentenceIdx) {
            return cached
        }

        // 2) Already in-flight — reuse the existing task (dedup)
        if let existingTask = fetchTasks[sentenceIdx] {
            return await existingTask.value
        }

        // 3) New request — create a task and store it for dedup
        let task = Task<(Data, Double)?, Never> { [weak self] in
            guard let self else { return nil }
            defer {
                Task { @MainActor [weak self] in
                    self?.fetchTasks.removeValue(forKey: sentenceIdx)
                }
            }
            return await self.fetchAudioOnce(sentenceIdx: sentenceIdx, priority: priority)
        }

        fetchTasks[sentenceIdx] = task
        return await task.value
    }

    // MARK: - Prefetch

    /// Eagerly fetch the next `prefetchWindow` sentences in waves of
    /// `maxConcurrentPrefetch` to ensure gapless playback.
    ///
    /// Mirrors the web app's "Prefetch Effect". Already-cached and in-flight
    /// sentences are skipped. Called after each sentence index change.
    private func prefetchAhead() {
        prefetchTask?.cancel()

        prefetchTask = Task { [weak self] in
            guard let self else { return }
            let len = self.sentences.count
            let startIdx = self.currentSentenceIndex

            // Collect sentences that actually need fetching
            var toFetch: [Int] = []
            for i in (startIdx + 1)...min(startIdx + self.prefetchWindow, len - 1) {
                guard i < len else { break }
                if !self.cache.has(i) && self.fetchTasks[i] == nil {
                    toFetch.append(i)
                }
            }

            // Dispatch in waves to avoid flooding the backend
            var waveStart = 0
            while waveStart < toFetch.count {
                if Task.isCancelled { break }

                let waveEnd = min(waveStart + self.maxConcurrentPrefetch, toFetch.count)
                let wave = Array(toFetch[waveStart..<waveEnd])

                // Fire all requests in the wave concurrently
                await withTaskGroup(of: Void.self) { group in
                    for idx in wave {
                        group.addTask { [weak self] in
                            _ = await self?.fetchAudio(sentenceIdx: idx, priority: .prefetch)
                        }
                    }
                }

                waveStart = waveEnd
            }
        }
    }

    // MARK: - Progress Timer

    /// Start a repeating timer that updates word-level progress ~10x per second.
    ///
    /// The timer reads `audioPlayer.currentTime` / `audioPlayer.duration` to compute
    /// a fractional progress, then maps it to a token count for per-word highlighting.
    /// Also ticks the remaining-time estimate for smooth countdown display.
    private func startProgressTimer() {
        stopProgressTimer()

        progressTimer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self, let player = self.audioPlayer, player.duration > 0 else { return }

                let progress = player.currentTime / player.duration
                let wordCount = SentenceSplitter.countTokens(self.sentences[self.currentSentenceIndex].text)
                // Quantize to word boundaries for crisp highlighting transitions
                self.currentWordProgress = floor(progress * Double(wordCount)) / Double(wordCount)

                // Tick the remaining-time estimate using real playback position
                self.updateEstimate(idx: self.currentSentenceIndex, elapsedSec: player.currentTime)
            }
        }
    }

    /// Stop and invalidate the progress timer.
    private func stopProgressTimer() {
        progressTimer?.invalidate()
        progressTimer = nil
    }

    // MARK: - Remaining Time Estimation

    /// Recompute the estimated remaining playback time.
    ///
    /// Uses a rolling average of the last 10 sentence durations to estimate
    /// time for all future sentences, plus the unplayed portion of the current
    /// sentence for a smoothly ticking display.
    ///
    /// - Parameters:
    ///   - idx: Current sentence index.
    ///   - elapsedSec: Seconds already played in the current sentence (0 at sentence start).
    private func updateEstimate(idx: Int, elapsedSec: Double = 0) {
        guard !durations.isEmpty && !sentences.isEmpty else {
            estimatedRemainingSeconds = -1
            return
        }

        // Use the last 10 durations for a stable average
        let recent = Array(durations.suffix(10))
        let avgMs = recent.reduce(0, +) / Double(recent.count)

        // Time for all sentences after the current one
        let futureSentences = sentences.count - idx - 1
        let futureSec = (avgMs * Double(futureSentences)) / 1000

        // Remaining portion of the current sentence
        let currentDurationMs = durations.last ?? avgMs
        let currentRemainingSec = max(0, currentDurationMs / 1000 - elapsedSec)

        estimatedRemainingSeconds = (futureSec + currentRemainingSec).rounded()
    }

    // MARK: - Task Management

    /// Cancel all in-flight fetch tasks and clear the dedup map.
    private func cancelAllFetchTasks() {
        for (_, task) in fetchTasks {
            task.cancel()
        }
        fetchTasks.removeAll()
    }

    // MARK: - Audio Session

    /// Configure AVAudioSession for background spoken audio playback.
    ///
    /// `.playback` category allows audio to continue when the app is backgrounded.
    /// `.spokenAudio` mode enables proper ducking behavior (other audio lowers its
    /// volume when TTS is speaking, rather than pausing completely).
    private func setupAudioSession() {
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playback, mode: .spokenAudio, options: [])
            try session.setActive(true)
        } catch {
            // Audio session setup failure is non-fatal — playback will still work
            // in the foreground, just not in the background.
            print("[TTSPlaybackEngine] Failed to configure audio session: \(error)")
        }
    }

    /// Deactivate the audio session when the engine is torn down.
    private func teardownAudioSession() {
        do {
            try AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        } catch {
            // Non-fatal — other apps may not resume their audio
            print("[TTSPlaybackEngine] Failed to deactivate audio session: \(error)")
        }
    }

    // MARK: - Now Playing (Lock Screen / Control Center)

    /// Set up initial Now Playing metadata with the item title and "Readio" artist.
    private func setupNowPlaying() {
        updateNowPlayingInfo()
    }

    /// Update the Now Playing info center with current playback state.
    ///
    /// Called when playback starts, pauses, or the sentence changes. Populates:
    /// - Title: item title
    /// - Artist: "Readio"
    /// - Elapsed time / Duration: from the current AVAudioPlayer
    /// - Playback rate: current speed (0 when paused)
    private func updateNowPlayingInfo() {
        var info = [String: Any]()
        info[MPMediaItemPropertyTitle] = itemTitle
        info[MPMediaItemPropertyArtist] = "Readio"

        if let player = audioPlayer {
            info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = player.currentTime
            info[MPMediaItemPropertyPlaybackDuration] = player.duration
            info[MPNowPlayingInfoPropertyPlaybackRate] = isPlaying ? speed : 0
        }

        // Show sentence progress in the "album" field as a breadcrumb
        info[MPMediaItemPropertyAlbumTitle] = "Sentence \(currentSentenceIndex + 1) / \(sentences.count)"

        MPNowPlayingInfoCenter.default().nowPlayingInfo = info
    }

    /// Clear all Now Playing metadata (called during cleanup).
    private func clearNowPlaying() {
        MPNowPlayingInfoCenter.default().nowPlayingInfo = nil
    }

    // MARK: - Remote Command Center (Lock Screen Controls)

    /// Register handlers for the lock screen / Control Center / AirPods controls.
    ///
    /// Supports: play, pause, toggle play/pause, next track (→ next sentence),
    /// previous track (→ previous sentence / restart current).
    private func setupRemoteCommands() {
        let center = MPRemoteCommandCenter.shared()

        center.playCommand.isEnabled = true
        center.playCommand.addTarget { [weak self] _ in
            Task { @MainActor in
                self?.play()
            }
            return .success
        }

        center.pauseCommand.isEnabled = true
        center.pauseCommand.addTarget { [weak self] _ in
            Task { @MainActor in
                self?.pause()
            }
            return .success
        }

        center.togglePlayPauseCommand.isEnabled = true
        center.togglePlayPauseCommand.addTarget { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                if self.isPlaying {
                    self.pause()
                } else {
                    self.play()
                }
            }
            return .success
        }

        center.nextTrackCommand.isEnabled = true
        center.nextTrackCommand.addTarget { [weak self] _ in
            Task { @MainActor in
                self?.nextSentence()
            }
            return .success
        }

        center.previousTrackCommand.isEnabled = true
        center.previousTrackCommand.addTarget { [weak self] _ in
            Task { @MainActor in
                self?.prevSentence()
            }
            return .success
        }
    }

    /// Remove all remote command handlers (called during cleanup to avoid
    /// dangling references to a deallocated engine).
    private func removeRemoteCommands() {
        let center = MPRemoteCommandCenter.shared()
        center.playCommand.removeTarget(nil)
        center.pauseCommand.removeTarget(nil)
        center.togglePlayPauseCommand.removeTarget(nil)
        center.nextTrackCommand.removeTarget(nil)
        center.previousTrackCommand.removeTarget(nil)
    }
}

// MARK: - Array Safe Subscript
/// Safe array subscript that returns nil for out-of-bounds indices.
/// Used throughout the engine to avoid index-out-of-range crashes when
/// the sentence list changes concurrently with playback.
private extension Array {
    subscript(safe index: Int) -> Element? {
        guard index >= 0 && index < count else { return nil }
        return self[index]
    }
}
