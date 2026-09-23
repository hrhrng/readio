import type {
  DiagnosticCategory,
  DiagnosticEventInput,
  DiagnosticFieldValue,
  RustCoreClient,
  SupportBundle,
} from "./rust-core.ts"

const DEFAULT_CAPACITY = 768

interface WebDiagnosticLogOptions {
  capacity?: number
  now?: () => number
  createOperationId?: () => string
}

export interface WebDiagnosticRecord {
  operationId: string
  component: string
  category: DiagnosticCategory
  name: string
  fields?: Record<string, DiagnosticFieldValue>
}

export interface WebSupportBundleOptions {
  core: Pick<RustCoreClient, "buildSupportBundle">
  appVersion: string
}

export interface SupportBundleFile {
  filename: string
  mediaType: "application/json"
  contents: string
}

/**
 * An ephemeral fact log, never a content log. Adapters may accidentally attach
 * an unsafe field while recording; the only export path still passes every
 * event through Rust Core's strict allowlist before bytes reach the user.
 */
export class WebDiagnosticLog {
  private readonly capacity: number
  private readonly now: () => number
  private readonly createOperationId: () => string
  private events: DiagnosticEventInput[] = []

  constructor(options: WebDiagnosticLogOptions = {}) {
    this.capacity = Math.max(1, Math.floor(options.capacity ?? DEFAULT_CAPACITY))
    this.now = options.now ?? Date.now
    this.createOperationId =
      options.createOperationId ?? (() => `op-${crypto.randomUUID()}`)
  }

  beginOperation(): string {
    return this.createOperationId()
  }

  record(record: WebDiagnosticRecord): void {
    this.events.push({
      timestamp_unix_ms: this.now(),
      operation_id: record.operationId,
      component: record.component,
      category: record.category,
      name: record.name,
      fields: { ...(record.fields ?? {}) },
    })
    if (this.events.length > this.capacity) {
      this.events.splice(0, this.events.length - this.capacity)
    }
  }

  snapshot(): DiagnosticEventInput[] {
    return this.events.map((event) => ({
      ...event,
      fields: { ...event.fields },
    }))
  }

  clear(): void {
    this.events = []
  }

  buildSupportBundle(options: WebSupportBundleOptions): Promise<SupportBundle> {
    return options.core.buildSupportBundle({
      generated_at_unix_ms: this.now(),
      platform: "web",
      app_version: options.appVersion,
      events: this.snapshot(),
    })
  }
}

export function supportBundleFile(bundle: SupportBundle): SupportBundleFile {
  const timestamp = new Date(bundle.generated_at_unix_ms)
    .toISOString()
    .replace(/[:.]/g, "-")
  return {
    filename: `readio-support-${timestamp}.json`,
    mediaType: "application/json",
    contents: `${JSON.stringify(bundle, null, 2)}\n`,
  }
}

export const webDiagnostics = new WebDiagnosticLog()
