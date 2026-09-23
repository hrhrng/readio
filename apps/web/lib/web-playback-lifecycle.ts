import type { PlaybackState } from "./rust-core"

export interface ClosableWebPlaybackExecutor {
  state(): Pick<PlaybackState, "status">
  pause(): Promise<void>
}

export interface DisposableWebPlaybackPlatform {
  dispose(): void
}

/**
 * Close a reader through the portable pause transition before releasing media.
 * The pause samples the active browser audio cursor and persists it through the
 * Rust effect protocol; disposing first would erase the only authoritative
 * position and make route changes resume from an older UI projection.
 */
export async function closeWebPlayback(
  executor: ClosableWebPlaybackExecutor,
  platform: DisposableWebPlaybackPlatform
): Promise<void> {
  try {
    const { status } = executor.state()
    if (status === "playing" || status === "loading") {
      await executor.pause()
    }
  } finally {
    platform.dispose()
  }
}
