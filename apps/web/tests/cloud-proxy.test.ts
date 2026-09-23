import assert from "node:assert/strict"
import test from "node:test"

test("the same-origin cloud proxy preserves auth and streams request and response bodies", async () => {
  const routeURL = new URL(
    "../app/api/cloud/[...path]/route.ts",
    import.meta.url
  )
  const route = await import(routeURL.href).catch((error: unknown) => {
    if (
      typeof error === "object" &&
      error !== null &&
      "code" in error &&
      error.code === "ERR_MODULE_NOT_FOUND"
    ) {
      return null
    }
    throw error
  })
  assert.ok(route, "the /api/cloud catch-all route must exist")

  const originalFetch = globalThis.fetch
  const originalCloudURL = process.env.READIO_CLOUD_URL
  const requestBody = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(new Uint8Array([1, 2, 3]))
      controller.close()
    },
  })
  const responseBody = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(new Uint8Array([4, 5, 6]))
      controller.close()
    },
  })
  let forwarded:
    | { input: RequestInfo | URL; init: RequestInit | undefined }
    | undefined

  process.env.READIO_CLOUD_URL = "https://cloud.readio.test/gateway/"
  globalThis.fetch = async (input, init) => {
    forwarded = { input, init }
    return new Response(responseBody, {
      status: 201,
      headers: { "x-readio-upstream": "streamed" },
    })
  }

  try {
    const request = new Request(
      "https://readio.test/api/cloud/v1/library/content/book/reader?mode=full",
      {
        method: "PUT",
        headers: {
          authorization: "Bearer token",
          cookie: "better-auth.session_token=session",
          "content-length": "3",
          host: "readio.test",
        },
        body: requestBody,
        duplex: "half",
      } as RequestInit & { duplex: "half" }
    )

    const response = await route.PUT(request)

    assert.equal(
      String(forwarded?.input),
      "https://cloud.readio.test/gateway/v1/library/content/book/reader?mode=full"
    )
    assert.strictEqual(forwarded?.init?.body, request.body)
    const headers = new Headers(forwarded?.init?.headers)
    assert.equal(headers.get("authorization"), "Bearer token")
    assert.equal(headers.get("cookie"), "better-auth.session_token=session")
    assert.equal(headers.has("host"), false)
    assert.equal(headers.has("content-length"), false)
    assert.equal(response.status, 201)
    assert.equal(response.headers.get("x-readio-upstream"), "streamed")
    assert.strictEqual(response.body, responseBody)
  } finally {
    globalThis.fetch = originalFetch
    if (originalCloudURL === undefined) delete process.env.READIO_CLOUD_URL
    else process.env.READIO_CLOUD_URL = originalCloudURL
  }
})
