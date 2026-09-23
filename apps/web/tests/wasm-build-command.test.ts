import assert from "node:assert/strict"
import { spawnSync } from "node:child_process"
import { fileURLToPath } from "node:url"
import test from "node:test"

test("the Web WASM bundle has one reproducible version-checked build target", () => {
  const repositoryRoot = fileURLToPath(new URL("../../..", import.meta.url))
  const result = spawnSync("make", ["-n", "wasm-web"], {
    cwd: repositoryRoot,
    encoding: "utf8",
  })
  const output = `${result.stdout}${result.stderr}`

  assert.equal(result.status, 0, output)
  assert.match(output, /wasm-bindgen.*0\.2\.126/)
  assert.match(
    output,
    /cargo build --release -p readio-wasm --target wasm32-unknown-unknown/
  )
  assert.match(
    output,
    /wasm-bindgen --target web --out-dir apps\/web\/wasm --out-name readio_wasm/
  )
})
