import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import test from "node:test"

import { JSDOM } from "jsdom"
import postcss, { type ChildNode, type Rule } from "postcss"

const GLOBALS_CSS = new URL("../app/globals.css", import.meta.url)

/**
 * JSDOM does not implement cascade layers. Flatten the production stylesheet
 * into browser layer order, then mark unlayered declarations important because
 * the CSS cascade ranks normal unlayered styles above normal layered styles.
 */
function cssForJSDOM(source: string): string {
  const parsed = postcss.parse(source)
  const output = postcss.root()
  const layers = new Map<string, ChildNode[]>()
  const unlayeredRules: Rule[] = []
  const rootVariables = new Map<string, string>()

  parsed.each((node) => {
    if (node.type === "atrule" && node.name === "layer" && node.nodes) {
      layers.set(
        node.params,
        node.nodes.map((child) => child.clone())
      )
      return
    }

    if (node.type === "rule") {
      if (node.selector === ":root") {
        node.walkDecls(/^--/, (declaration) => {
          rootVariables.set(declaration.prop, declaration.value)
        })
        output.append(node.clone())
      } else {
        unlayeredRules.push(node.clone())
      }
    }
  })

  for (const layerName of ["base", "components", "utilities"]) {
    for (const node of layers.get(layerName) ?? []) output.append(node)
  }

  for (const rule of unlayeredRules) {
    rule.walkDecls((declaration) => {
      declaration.important = true
    })
    output.append(rule)
  }

  const resolveVariables = (value: string, seen = new Set<string>()): string =>
    value.replace(/var\((--[\w-]+)\)/g, (match, variable: string) => {
      if (seen.has(variable)) return match

      const replacement = rootVariables.get(variable)
      if (!replacement) return match

      return resolveVariables(replacement, new Set([...seen, variable]))
    })

  output.walkDecls((declaration) => {
    declaration.value = resolveVariables(declaration.value)
  })

  return output.toString()
}

test("standard primary buttons keep a 14px product label", () => {
  const css = cssForJSDOM(readFileSync(GLOBALS_CSS, "utf8"))
  const dom = new JSDOM(
    `<style>${css}</style><button class="ui-button ui-button--primary">Sign in</button>`
  )
  const button = dom.window.document.querySelector("button")

  assert.ok(button)
  assert.equal(dom.window.getComputedStyle(button).fontSize, "14px")
})

test("sidebar selection and standard form controls share the 40px POC rhythm", () => {
  const css = cssForJSDOM(readFileSync(GLOBALS_CSS, "utf8"))
  const dom = new JSDOM(
    `<style>${css}</style>
     <a class="nav-item" data-active="true">Library</a>
     <button class="ui-button ui-button--primary">Import</button>
     <input class="ui-input" />
     <button class="ui-select-trigger">Sort</button>`
  )
  const selectors = [
    ".nav-item",
    ".ui-button",
    ".ui-input",
    ".ui-select-trigger",
  ]
  const heights = selectors.map((selector) => {
    const element = dom.window.document.querySelector(selector)
    assert.ok(element)
    return dom.window.getComputedStyle(element).minHeight
  })

  assert.deepEqual(heights, ["40px", "40px", "40px", "40px"])
})
