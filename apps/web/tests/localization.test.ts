import assert from "node:assert/strict"
import test from "node:test"

import {
  createTranslator,
  normalizeLocalePreference,
  resolveLocalePreference,
  resolveSupportedLocale,
  translationKeys,
} from "../lib/localization.ts"

test("Web locale resolution maps Simplified Chinese variants and falls back to English", () => {
  assert.equal(resolveSupportedLocale(["zh-CN"]), "zh-Hans")
  assert.equal(resolveSupportedLocale(["zh-Hans-CN"]), "zh-Hans")
  assert.equal(resolveSupportedLocale(["en-GB"]), "en")
  assert.equal(resolveSupportedLocale(["fr-FR"]), "en")
})

test("Web translations interpolate values without changing user content", () => {
  const zh = createTranslator("zh-Hans")
  const en = createTranslator("en")

  assert.equal(zh("library.bookCount", { count: 2 }), "2 本书")
  assert.equal(en("library.bookCount", { count: 1 }), "1 book")
  assert.equal(en("library.bookCount", { count: 2 }), "2 books")
  assert.equal(
    zh("library.removeLinkedMessage", { title: "Design Systems 设计系统" }),
    "“Design Systems 设计系统”将从 Readio 书库中移除，设备上的链接源文件会保留。"
  )
  assert.equal(
    zh("search.noResultsDescription", { query: "Rust" }),
    "没有与“Rust”匹配的书库内容。"
  )
  assert.equal(
    zh("reader.chapterPosition", { current: 2, total: 5 }),
    "第 2 章，共 5 章"
  )
})

test("English and Chinese catalogs expose the same registered UI keys", () => {
  const { en, zhHans } = translationKeys()
  assert.deepEqual(zhHans, en)
})

test("An explicit Web language overrides the browser while system follows it", () => {
  assert.equal(resolveLocalePreference("en", ["zh-CN"]), "en")
  assert.equal(resolveLocalePreference("zh-Hans", ["en-US"]), "zh-Hans")
  assert.equal(resolveLocalePreference("system", ["zh-CN"]), "zh-Hans")
})

test("Invalid persisted Web language preferences return to the system default", () => {
  assert.equal(normalizeLocalePreference("zh-Hans"), "zh-Hans")
  assert.equal(normalizeLocalePreference("en"), "en")
  assert.equal(normalizeLocalePreference("fr"), "system")
  assert.equal(normalizeLocalePreference(undefined), "system")
})
