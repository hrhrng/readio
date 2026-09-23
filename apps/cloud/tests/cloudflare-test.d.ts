/// <reference types="@cloudflare/vitest-pool-workers/types" />

export {}

declare module "cloudflare:test" {
  interface ProvidedEnv extends Env {
    TEST_MIGRATIONS: D1Migration[]
  }
}
