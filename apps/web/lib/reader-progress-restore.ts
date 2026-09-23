/**
 * Owns the lifetime of an asynchronous cursor restore across React effects.
 * A cancelled attempt releases its key for retry; only a completed attempt
 * closes that document revision permanently.
 */
export class ReaderProgressRestoreGate {
  private key: string | null = null
  private activeAttempt: number | null = null
  private completed = false
  private nextAttempt = 0

  begin(key = "default"): number | null {
    if (this.key !== key) {
      this.key = key
      this.activeAttempt = null
      this.completed = false
    }
    if (this.completed || this.activeAttempt !== null) return null
    this.activeAttempt = ++this.nextAttempt
    return this.activeAttempt
  }

  cancel(attempt: number): void {
    if (this.activeAttempt === attempt) this.activeAttempt = null
  }

  complete(attempt: number): void {
    if (this.activeAttempt !== attempt) return
    this.activeAttempt = null
    this.completed = true
  }
}
