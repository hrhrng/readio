/**
 * Next preserves an encoded dynamic segment when it crosses the client-page
 * boundary. Decode it once here so every reader subsystem receives the same
 * canonical item identity that Rust and the local replica use.
 */
export function resolvePlayerItemId(routeId: string): string {
  return decodeURIComponent(routeId)
}
