import path from "node:path"
import { fileURLToPath } from "node:url"

import {
  cloudflareTest,
  readD1Migrations,
} from "@cloudflare/vitest-pool-workers"
import { defineConfig } from "vitest/config"

const directory = path.dirname(fileURLToPath(import.meta.url))
const testSecrets = {
  BETTER_AUTH_SECRET: "test-secret-that-is-longer-than-thirty-two-bytes",
  MINIMAX_API_KEY: "test-key",
  MINIMAX_GROUP_ID: "",
}

// Wrangler validates required secret names before Miniflare receives its
// bindings. These process-local fixtures keep tests hermetic and warning-free;
// production values still come only from `wrangler secret put`.
for (const [name, value] of Object.entries(testSecrets)) {
  process.env[name] ??= value
}

export default defineConfig({
  plugins: [
    cloudflareTest(async () => ({
      main: path.join(directory, "tests/test-worker.ts"),
      wrangler: { configPath: path.join(directory, "wrangler.jsonc") },
      miniflare: {
        bindings: {
          TEST_MIGRATIONS: await readD1Migrations(path.join(directory, "migrations")),
          ...testSecrets,
        },
        r2Buckets: ["MEDIA"],
        queueProducers: ["TTS_QUEUE"],
      },
    })),
  ],
  test: {
    setupFiles: ["./tests/apply-migrations.ts"],
    include: ["tests/**/*.worker.test.ts"],
  },
})
