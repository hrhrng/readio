import { describe, expect, it } from "vitest"

import { RustStoragePlanner } from "../src/rust-storage-planner"

describe("Rust WASM storage planner", () => {
  it("executes the shared Core transition inside workerd", () => {
    const planner = new RustStoragePlanner()
    const plan = planner.plan(
      { entities: [], mutations: [] },
      {
        deviceId: "worker-a",
        mutations: [
          {
            mutationId: "m-1",
            entityKind: "setting",
            entityId: "theme",
            operation: "upsert",
            baseVersion: 0,
            clientTimestampUnixMs: 1,
            payload: { value: "dark" },
          },
        ],
      },
      10,
      0,
    )

    expect(plan).toMatchObject({
      cursor: 1,
      results: [{ mutation_id: "m-1", status: "applied", version: 1 }],
      writes: [{ expected_version: 0, record: { version: 1, cursor: 1 } }],
    })
  })
})
