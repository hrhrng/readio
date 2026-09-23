const PROXY_PREFIX = "/api/cloud"
const HOP_BY_HOP_HEADERS = [
  "connection",
  "content-length",
  "host",
  "keep-alive",
  "proxy-authenticate",
  "proxy-authorization",
  "te",
  "trailer",
  "transfer-encoding",
  "upgrade",
]

async function proxy(request: Request): Promise<Response> {
  const requestURL = new URL(request.url)
  const upstream = cloudURL(requestURL)
  const headers = new Headers(request.headers)
  for (const name of HOP_BY_HOP_HEADERS) headers.delete(name)

  const body =
    request.method === "GET" || request.method === "HEAD"
      ? undefined
      : request.body
  const init: RequestInit & { duplex?: "half" } = {
    method: request.method,
    headers,
    body,
    redirect: "manual",
    signal: request.signal,
  }
  if (body) init.duplex = "half"

  try {
    return await fetch(upstream, init)
  } catch {
    return Response.json(
      {
        error: {
          code: "cloud_unavailable",
          message: "Cloud sync is unavailable. Local reading still works.",
        },
      },
      { status: 502 }
    )
  }
}

function cloudURL(requestURL: URL): URL {
  const cloudOrigin =
    process.env.READIO_CLOUD_URL ||
    process.env.NEXT_PUBLIC_READIO_CLOUD_URL ||
    "http://127.0.0.1:8787"
  const upstream = new URL(cloudOrigin)
  const suffix = requestURL.pathname.startsWith(`${PROXY_PREFIX}/`)
    ? requestURL.pathname.slice(PROXY_PREFIX.length)
    : "/"
  upstream.pathname = `${upstream.pathname.replace(/\/+$/, "")}${suffix}`
  upstream.search = requestURL.search
  return upstream
}

export const DELETE = proxy
export const GET = proxy
export const HEAD = proxy
export const OPTIONS = proxy
export const PATCH = proxy
export const POST = proxy
export const PUT = proxy
