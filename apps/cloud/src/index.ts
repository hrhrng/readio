import { createCloudApp } from "./app"
import { createAuth } from "./auth"
import { CloudTTSService, type TTSQueueMessage } from "./cloud-tts-service"
import { D1SyncService } from "./d1-sync-service"
import { D1ReplicaService } from "./d1-replica-service"
import { D1LibraryContentService } from "./d1-library-content-service"
import { RustStoragePlanner } from "./rust-storage-planner"
import { consumeTTSBatch } from "./tts-consumer"
import type { TTSSynthesisOptions } from "./tts-providers"

function workersAIRunner(ai: Ai): TTSSynthesisOptions["runWorkersAI"] {
  return async (model, input, options) => {
    const result = await ai.run(model, input, options)
    if (result instanceof Response) return result
    throw new Error("workers_ai_raw_response_missing")
  }
}

function trustedOrigins(env: Env): Set<string> {
  return new Set(
    env.TRUSTED_ORIGINS.split(",")
      .map((origin) => origin.trim())
      .filter(Boolean),
  )
}

function corsHeaders(request: Request, env: Env): Headers {
  const headers = new Headers()
  const origin = request.headers.get("origin")
  if (origin && trustedOrigins(env).has(origin)) {
    headers.set("access-control-allow-origin", origin)
    headers.set("access-control-allow-credentials", "true")
    headers.set("vary", "origin")
  }
  headers.set(
    "access-control-allow-headers",
    "authorization, content-type, x-readio-device-id, x-readio-replica-cursor, x-readio-content-sha256",
  )
  headers.set("access-control-allow-methods", "GET, HEAD, POST, PUT, OPTIONS")
  headers.set(
    "access-control-expose-headers",
    "etag, set-auth-token, x-readio-replica-cursor, x-readio-content-sha256",
  )
  return headers
}

function withCors(response: Response, request: Request, env: Env): Response {
  const result = new Response(response.body, response)
  for (const [name, value] of corsHeaders(request, env)) {
    result.headers.set(name, value)
  }
  return result
}

function app(env: Env) {
  const auth = createAuth(env)
  const tts = new CloudTTSService(
    env.DB,
    env.TTS_QUEUE as Queue<TTSQueueMessage>,
    env.MEDIA,
  )
  return createCloudApp({
    async authenticate(request) {
      const session = await auth.api.getSession({ headers: request.headers })
      return session ? { userId: session.user.id } : null
    },
    authHandler: (request) => auth.handler(request),
    fetchURL: (target, init) => fetch(target, init),
    sync: new D1SyncService(env.DB, new RustStoragePlanner()),
    replica: new D1ReplicaService(env.DB, env.MEDIA),
    content: new D1LibraryContentService(env.DB, env.MEDIA),
    tts,
  })
}

export default {
  async fetch(
    request: Request,
    env: Env,
    _context: ExecutionContext,
  ): Promise<Response> {
    if (request.method === "OPTIONS") {
      const origin = request.headers.get("origin")
      if (origin && !trustedOrigins(env).has(origin)) {
        return new Response(null, { status: 403 })
      }
      return new Response(null, { status: 204, headers: corsHeaders(request, env) })
    }

    return withCors(await app(env).fetch(request), request, env)
  },

  async queue(batch: MessageBatch<TTSQueueMessage>, env: Env): Promise<void> {
    await consumeTTSBatch(batch, env, workersAIRunner(env.AI))
  },
} satisfies ExportedHandler<Env, TTSQueueMessage>
