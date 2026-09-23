import assert from "node:assert/strict"
import { spawnSync } from "node:child_process"
import test from "node:test"

test("the web application type-checks as one integrated reader surface", () => {
  const result = spawnSync(
    process.execPath,
    ["node_modules/typescript/bin/tsc", "--noEmit", "--pretty", "false"],
    {
      cwd: process.cwd(),
      encoding: "utf8",
    }
  )

  assert.equal(
    result.status,
    0,
    [result.stdout, result.stderr].filter(Boolean).join("\n")
  )
})
