import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import test from "node:test"

const middleware = readFileSync(new URL("../middleware.ts", import.meta.url), "utf8")
const authRoute = readFileSync(
  new URL("../app/api/auth/[...all]/route.ts", import.meta.url),
  "utf8",
)
const popover = readFileSync(
  new URL("../components/user-popover.tsx", import.meta.url),
  "utf8",
)
const packageJSON = JSON.parse(
  readFileSync(new URL("../package.json", import.meta.url), "utf8"),
) as { dependencies: Record<string, string> }
const tuiPage = readFileSync(new URL("../app/tui/page.tsx", import.meta.url), "utf8")
const authClient = readFileSync(new URL("../lib/auth-client.ts", import.meta.url), "utf8")
const devicePage = readFileSync(new URL("../app/device/page.tsx", import.meta.url), "utf8")
const deviceApprovalPage = readFileSync(
  new URL("../app/device/approve/page.tsx", import.meta.url),
  "utf8",
)

test("the web library is local-first and never redirects anonymous readers", () => {
  assert.doesNotMatch(middleware, /NextResponse\.redirect/)
  assert.doesNotMatch(middleware, /better-auth\.session_token/)
  assert.match(popover, /Local library/)
  assert.match(popover, /Sign in to sync/)
})

test("web auth proxies to the cloud Worker instead of opening a local SQLite database", () => {
  assert.match(authRoute, /READIO_CLOUD_URL/)
  assert.doesNotMatch(authRoute, /toNextJsHandler/)
  assert.equal(packageJSON.dependencies["better-sqlite3"], undefined)
})

test("the TUI landing page opens the anonymous web app", () => {
  assert.doesNotMatch(tuiPage, /href="\/login"[\s\S]{0,160}>\s*Web app/)
})

test("TUI and native apps can complete device authorization in the web app", () => {
  assert.match(authClient, /deviceAuthorizationClient/)
  assert.match(devicePage, /user_code/)
  assert.match(deviceApprovalPage, /device\.approve/)
  assert.match(deviceApprovalPage, /t\("device\.approveTitle"\)/)
})
