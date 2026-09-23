import { env } from "cloudflare:workers"
import {
  createExecutionContext,
  waitOnExecutionContext,
} from "cloudflare:test"
import { describe, expect, it } from "vitest"

import worker from "../src/index"

describe("deployed Worker boundary", () => {
  it("allows trusted Web clients to preflight streaming replica snapshots", async () => {
    const context = createExecutionContext()
    const response = await worker.fetch(
      new Request("https://cloud.read-io.com/v1/replica/snapshot", {
        method: "OPTIONS",
        headers: {
          origin: "http://localhost:3000",
          "access-control-request-method": "PUT",
          "access-control-request-headers":
            "authorization,content-type,x-readio-device-id,x-readio-replica-cursor",
        },
      }),
      env,
      context,
    )
    await waitOnExecutionContext(context)

    expect(response.status).toBe(204)
    expect(response.headers.get("access-control-allow-methods")).toContain("PUT")
    expect(response.headers.get("access-control-allow-headers")).toContain(
      "x-readio-replica-cursor",
    )
  })

  it("keeps local use anonymous while protecting sync", async () => {
    const context = createExecutionContext()
    const capabilities = await worker.fetch(
      new Request("https://cloud.read-io.com/v1/capabilities"),
      env,
      context,
    )
    const sync = await worker.fetch(
      new Request("https://cloud.read-io.com/v1/sync/push", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ deviceId: "web-a", mutations: [] }),
      }),
      env,
      context,
    )
    await waitOnExecutionContext(context)

    expect(await capabilities.json()).toMatchObject({
      localUseRequiresAuth: false,
      syncRequiresAuth: true,
    })
    expect(sync.status).toBe(401)
    expect(await sync.json()).toMatchObject({ error: { code: "auth_required" } })
  })

  it("offers RFC 8628 device login for TUI and native clients", async () => {
    const context = createExecutionContext()
    const response = await worker.fetch(
      new Request("https://cloud.read-io.com/api/auth/device/code", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          client_id: "readio-tui",
          scope: "openid profile email",
        }),
      }),
      env,
      context,
    )
    await waitOnExecutionContext(context)

    expect(response.status).toBe(200)
    expect(await response.json()).toMatchObject({
      user_code: expect.any(String),
      device_code: expect.any(String),
      verification_uri: expect.stringContaining("/device"),
    })
  })
})
