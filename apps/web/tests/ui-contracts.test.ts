import assert from "node:assert/strict"
import test from "node:test"

import {
  resolveFeedback,
  resolvePageLayout,
} from "../lib/ui-contracts.ts"
import { toastReducer, type ToastNotice } from "../lib/toast-store.ts"

test("library uses a spacious shelf while search keeps a dense result list", () => {
  const library = resolvePageLayout("library")
  const search = resolvePageLayout("search")

  assert.deepEqual(library, {
    frame: "overview",
    density: "comfortable",
    header: "sticky",
    alignment: "edges",
  })
  assert.deepEqual(search, {
    frame: "list",
    density: "compact",
    header: "sticky",
    alignment: "edges",
  })
})

test("overview and reader routes keep distinct content-width contracts", () => {
  assert.deepEqual(resolvePageLayout("home"), {
    frame: "overview",
    density: "comfortable",
    header: "sticky",
    alignment: "edges",
  })
  assert.deepEqual(resolvePageLayout("reader"), {
    frame: "reader",
    density: "comfortable",
    header: "fixed",
    alignment: "x-center",
  })
})

test("transient library outcomes map to polite global toasts", () => {
  assert.deepEqual(resolveFeedback("library.imported"), {
    tone: "success",
    surface: "toast",
    politeness: "polite",
  })
  assert.deepEqual(resolveFeedback("library.deleted"), {
    tone: "neutral",
    surface: "toast",
    politeness: "polite",
  })
})

test("bookmark completion and recovery use distinct global surfaces", () => {
  assert.deepEqual(resolveFeedback("bookmark.saved"), {
    tone: "success",
    surface: "toast",
    politeness: "polite",
  })
  assert.deepEqual(resolveFeedback("bookmark.deleted"), {
    tone: "neutral",
    surface: "toast",
    politeness: "polite",
  })
  assert.deepEqual(resolveFeedback("bookmark.failed"), {
    tone: "danger",
    surface: "inline",
    politeness: "assertive",
  })
})

test("irreversible bookmark deletion asks for confirmation before mutation", () => {
  assert.deepEqual(resolveFeedback("bookmark.delete_requested"), {
    tone: "danger",
    surface: "alert-dialog",
    politeness: "assertive",
  })
})

test("blocking failures stay next to the task that can recover them", () => {
  assert.deepEqual(resolveFeedback("auth.failed"), {
    tone: "danger",
    surface: "inline",
    politeness: "assertive",
  })
  assert.deepEqual(resolveFeedback("tts.failed"), {
    tone: "danger",
    surface: "banner",
    politeness: "assertive",
  })
  assert.deepEqual(resolveFeedback("library.import_failed"), {
    tone: "danger",
    surface: "inline",
    politeness: "assertive",
  })
})

test("global toast queue keeps the newest three notices and supports dismissal", () => {
  const notices = ["one", "two", "three", "four"].reduce<ToastNotice[]>(
    (state, message, index) =>
      toastReducer(state, {
        type: "push",
        toast: {
          id: String(index),
          message,
          tone: "neutral",
          politeness: "polite",
        },
      }),
    []
  )

  assert.deepEqual(
    notices.map(({ message }) => message),
    ["two", "three", "four"]
  )
  assert.deepEqual(
    toastReducer(notices, { type: "dismiss", id: "2" }).map(
      ({ message }) => message
    ),
    ["two", "four"]
  )
})
