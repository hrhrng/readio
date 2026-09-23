export type SupportedMediaSessionAction =
  | "play"
  | "pause"
  | "stop"
  | "nexttrack"
  | "previoustrack"

export interface MediaSessionPort {
  playbackState: MediaSessionPlaybackState
  metadata: MediaMetadata | null
  setPositionState?(state?: MediaPositionState): void
  setActionHandler(
    action: SupportedMediaSessionAction,
    handler: (() => void) | null
  ): void
}

export interface MediaSessionPlayerCommands {
  play(): void
  pause(): void
  nextSentence(): void
  prevSentence(): void
}

export interface MediaSessionProjection {
  title: string
  artist: string
  artwork: string | null
  isPlaying: boolean
  durationSeconds?: number | null
  positionSeconds?: number
  playbackRate?: number
}

type MediaMetadataFactory = (metadata: MediaMetadataInit) => MediaMetadata

const ACTIONS: SupportedMediaSessionAction[] = [
  "play",
  "pause",
  "stop",
  "nexttrack",
  "previoustrack",
]

/** Browser capability adapter; all state transitions remain in Rust/player. */
export class BrowserMediaSessionController {
  private connected = false

  constructor(
    private readonly session: MediaSessionPort,
    private readonly createMetadata: MediaMetadataFactory = (metadata) =>
      new MediaMetadata(metadata)
  ) {}

  connect(commands: MediaSessionPlayerCommands): void {
    if (this.connected) this.removeHandlers()
    this.session.setActionHandler("play", commands.play)
    this.session.setActionHandler("pause", commands.pause)
    this.session.setActionHandler("stop", commands.pause)
    this.session.setActionHandler("nexttrack", commands.nextSentence)
    this.session.setActionHandler("previoustrack", commands.prevSentence)
    this.connected = true
  }

  update(projection: MediaSessionProjection): void {
    const artwork = projection.artwork
      ? [{ src: projection.artwork }]
      : undefined
    this.session.metadata = this.createMetadata({
      title: projection.title,
      artist: projection.artist,
      album: "Readio",
      artwork,
    })
    this.session.playbackState = projection.isPlaying ? "playing" : "paused"
    this.updatePosition(projection)
  }

  disconnect(): void {
    this.removeHandlers()
    this.session.playbackState = "none"
    this.session.metadata = null
    this.session.setPositionState?.()
    this.connected = false
  }

  private removeHandlers(): void {
    for (const action of ACTIONS) this.session.setActionHandler(action, null)
  }

  private updatePosition(projection: MediaSessionProjection): void {
    if (!this.session.setPositionState) return
    const duration = projection.durationSeconds
    const position = projection.positionSeconds
    const playbackRate = projection.playbackRate
    if (
      duration == null ||
      position == null ||
      playbackRate == null ||
      !Number.isFinite(duration) ||
      !Number.isFinite(position) ||
      !Number.isFinite(playbackRate) ||
      duration <= 0 ||
      playbackRate <= 0
    ) {
      this.session.setPositionState()
      return
    }
    this.session.setPositionState({
      duration,
      position: Math.min(duration, Math.max(0, position)),
      playbackRate,
    })
  }
}
