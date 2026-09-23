import assert from "node:assert/strict"
import test from "node:test"
import React, { act } from "react"
import { createRoot } from "react-dom/client"
import { JSDOM } from "jsdom"
import { SWRConfig } from "swr"

import { ImportDialog } from "../components/import-dialog.tsx"
import { LocaleProvider } from "../components/locale-provider.tsx"
import { ToastProvider } from "../components/toast-provider.tsx"

;(globalThis as typeof globalThis & { React: typeof React }).React = React

test("an in-flight Web import exposes real progress and a working cancel action", async () => {
  const dom = new JSDOM(
    "<!doctype html><html><body><div id='root'></div></body></html>",
    { url: "http://localhost" }
  )
  const previous = {
    window: globalThis.window,
    document: globalThis.document,
    navigator: globalThis.navigator,
    HTMLElement: globalThis.HTMLElement,
    Node: globalThis.Node,
    Event: globalThis.Event,
    MouseEvent: globalThis.MouseEvent,
    fetch: globalThis.fetch,
  }
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    navigator: dom.window.navigator,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    Event: dom.window.Event,
    MouseEvent: dom.window.MouseEvent,
    IS_REACT_ACT_ENVIRONMENT: true,
  })

  let requestSignal: AbortSignal | null = null
  globalThis.fetch = async (_input, init) => {
    requestSignal = init?.signal ?? null
    if (!requestSignal) throw new Error("import UI omitted its cancellation signal")
    return await new Promise<Response>((_resolve, reject) => {
      requestSignal?.addEventListener(
        "abort",
        () => reject(new DOMException("Import cancelled", "AbortError")),
        { once: true }
      )
    })
  }

  const root = createRoot(dom.window.document.querySelector("#root")!)
  try {
    await act(async () => {
      root.render(
        React.createElement(
          SWRConfig,
          { value: { provider: () => new Map() } },
          React.createElement(
            LocaleProvider,
            { initialLocale: "en", initialPreference: "en" },
            React.createElement(
              ToastProvider,
              null,
              React.createElement(ImportDialog, {
                open: true,
                onClose() {},
              })
            )
          )
        )
      )
    })

    const tabs = Array.from(
      dom.window.document.querySelectorAll<HTMLButtonElement>('[role="tab"]')
    )
    await act(async () => tabs[1]?.click())

    const input = dom.window.document.querySelector<HTMLInputElement>(
      'input[type="url"]'
    )!
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(
        dom.window.HTMLInputElement.prototype,
        "value"
      )?.set
      setter?.call(input, "https://example.com/slow")
      input.dispatchEvent(new dom.window.Event("input", { bubbles: true }))
    })

    const importButton = Array.from(
      dom.window.document.querySelectorAll<HTMLButtonElement>("button")
    ).find((button) => button.textContent?.includes("Import URL"))!
    await act(async () => {
      importButton.click()
      await Promise.resolve()
    })

    const progress = dom.window.document.querySelector("progress")
    const cancelButton = Array.from(
      dom.window.document.querySelectorAll<HTMLButtonElement>("button")
    ).find((button) => button.textContent?.includes("Cancel import"))
    assert.equal(progress?.getAttribute("max"), "4")
    assert.equal(progress?.getAttribute("value"), "0")
    assert.ok(cancelButton)

    await act(async () => {
      cancelButton?.click()
      await Promise.resolve()
    })
    assert.equal(requestSignal?.aborted, true)
    assert.match(dom.window.document.body.textContent ?? "", /Import URL/)
    assert.doesNotMatch(dom.window.document.body.textContent ?? "", /Import failed/i)
  } finally {
    await act(async () => root.unmount())
    Object.assign(globalThis, previous)
    delete (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean })
      .IS_REACT_ACT_ENVIRONMENT
  }
})
