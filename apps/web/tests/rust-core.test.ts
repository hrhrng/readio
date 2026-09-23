import assert from "node:assert/strict"
import test from "node:test"

import { createRustCoreClient } from "../lib/rust-core.ts"

test("the web boundary delegates support-bundle redaction to Rust", async () => {
  const request = {
    generated_at_unix_ms: 100,
    platform: "web",
    app_version: "0.2.0",
    events: [
      {
        timestamp_unix_ms: 90,
        operation_id: "op-shared",
        component: "web-reader",
        category: "playback" as const,
        name: "clock_sample",
        fields: {
          media_position_ms: 250,
          book_text: "Private words",
        },
      },
    ],
  }
  const core = createRustCoreClient(async () => ({
    build_support_bundle(requestJSON: string) {
      assert.deepEqual(JSON.parse(requestJSON), request)
      return JSON.stringify({
        schema_version: 1,
        generated_at_unix_ms: 100,
        platform: "web",
        app_version: "0.2.0",
        redaction: {
          content_included: false,
          secrets_included: false,
          dropped_field_count: 1,
          dropped_event_count: 0,
        },
        events: [
          {
            ...request.events[0],
            fields: { media_position_ms: 250 },
          },
        ],
      })
    },
    create_playback_state() {
      throw new Error("not used")
    },
    reduce_playback() {
      throw new Error("not used")
    },
    parse_html_document() {
      throw new Error("not used")
    },
  }))

  const bundle = await core.buildSupportBundle(request)

  assert.equal(bundle.events[0]?.operation_id, "op-shared")
  assert.deepEqual(bundle.events[0]?.fields, { media_position_ms: 250 })
  assert.equal(bundle.redaction.content_included, false)
})

test("the web boundary delegates honest reading progress and seek to Rust", async () => {
  const core = createRustCoreClient(async () => ({
    create_playback_state() {
      throw new Error("not used")
    },
    reduce_playback() {
      throw new Error("not used")
    },
    parse_html_document() {
      throw new Error("not used")
    },
    project_reading_progress(requestJSON: string) {
      const request = JSON.parse(requestJSON)
      assert.deepEqual(request.segment_character_counts, [2, 8])
      assert.equal(request.timing.type, "unknown")
      return JSON.stringify({
        char_offset: 7,
        char_count: 11,
        progress_basis_points: 6363,
        remaining_characters: 4,
        remaining_time: { kind: "unknown", seconds: null, sample_count: 0 },
      })
    },
    resolve_reading_seek(requestJSON: string) {
      assert.deepEqual(JSON.parse(requestJSON), {
        segment_character_counts: [1, 9],
        progress_basis_points: 5000,
      })
      return JSON.stringify({
        segment: 1,
        segment_progress_basis_points: 3333,
        char_offset: 5,
      })
    },
  }))

  const projection = await core.projectReadingProgress({
    segment_character_counts: [2, 8],
    current_segment: 1,
    current_segment_progress_basis_points: 5000,
    timing: { type: "unknown" },
  })
  assert.equal(projection.progress_basis_points, 6363)
  assert.equal(projection.remaining_time.seconds, null)
  assert.deepEqual(await core.resolveReadingSeek([1, 9], 5000), {
    segment: 1,
    segment_progress_basis_points: 3333,
    char_offset: 5,
  })
})

test("the web boundary delegates server model catalog normalization to Rust", async () => {
  const wasm = {
    normalize_tts_model_catalog(catalogJSON: string) {
      assert.deepEqual(JSON.parse(catalogJSON), {
        schemaVersion: 1,
        defaultModelId: "missing",
        models: [
          {
            id: " workers-ai/melotts ",
            provider: " Workers-AI ",
            label: " MeloTTS ",
            languages: ["ZH_hans_cn"],
            voices: [{ id: " default ", label: " Default " }],
            defaultVoiceId: "missing",
          },
        ],
      })
      return JSON.stringify({
        schemaVersion: 1,
        defaultModelId: "workers-ai/melotts",
        models: [
          {
            id: "workers-ai/melotts",
            provider: "workers-ai",
            label: "MeloTTS",
            languages: ["zh-Hans-CN"],
            voices: [{ id: "default", label: "Default" }],
            defaultVoiceId: "default",
          },
        ],
      })
    },
    create_playback_state() {
      throw new Error("not used")
    },
    reduce_playback() {
      throw new Error("not used")
    },
    parse_html_document() {
      throw new Error("not used")
    },
  }
  const core = createRustCoreClient(async () => wasm)
  const normalize = Reflect.get(core, "normalizeTTSModelCatalog") as unknown as
    | ((catalog: Record<string, unknown>) => Promise<Record<string, unknown>>)
    | undefined

  assert.equal(typeof normalize, "function")
  assert.deepEqual(
    await normalize!({
      schemaVersion: 1,
      defaultModelId: "missing",
      models: [
        {
          id: " workers-ai/melotts ",
          provider: " Workers-AI ",
          label: " MeloTTS ",
          languages: ["ZH_hans_cn"],
          voices: [{ id: " default ", label: " Default " }],
          defaultVoiceId: "missing",
        },
      ],
    }),
    {
      schemaVersion: 1,
      defaultModelId: "workers-ai/melotts",
      models: [
        {
          id: "workers-ai/melotts",
          provider: "workers-ai",
          label: "MeloTTS",
          languages: ["zh-Hans-CN"],
          voices: [{ id: "default", label: "Default" }],
          defaultVoiceId: "default",
        },
      ],
    }
  )
})

test("the web boundary decodes the locale-specific virtual sample from WASM", async () => {
  const core = createRustCoreClient(async () => ({
    builtin_sample(languageTag: string) {
      assert.equal(languageTag, "zh-Hans-CN")
      return JSON.stringify({
        item_id: "readio:sample:v1",
        legacy_item_ids: ["6b4121313cc4a35d"],
        locale: "zh",
        title: "注意力的形状",
        author: "readio 示例",
        source: "readio://sample",
        content_format: "plaintext",
        content: "共享正文",
        markdown_source: "# 共享正文",
        is_builtin_sample: true,
      })
    },
    create_playback_state() {
      throw new Error("not used")
    },
    reduce_playback() {
      throw new Error("not used")
    },
    parse_html_document() {
      throw new Error("not used")
    },
  }))

  assert.deepEqual(await core.builtinSample("zh-Hans-CN"), {
    item_id: "readio:sample:v1",
    legacy_item_ids: ["6b4121313cc4a35d"],
    locale: "zh",
    title: "注意力的形状",
    author: "readio 示例",
    source: "readio://sample",
    content_format: "plaintext",
    content: "共享正文",
    markdown_source: "# 共享正文",
    is_builtin_sample: true,
  })
})

test("the web boundary decodes a playback state returned by WASM", async () => {
  const core = createRustCoreClient(async () => ({
    create_playback_state(sentenceCount: number) {
      if (sentenceCount !== 2) {
        throw new Error("unexpected sentence count")
      }
      return JSON.stringify({
        status: "idle",
        current_sentence: 0,
        sentence_count: 2,
        generation: 0,
        current_duration_ms: null,
        current_position_ms: 0,
        last_tick_monotonic_ms: null,
        sleep_deadline_unix_ms: null,
        error: null,
      })
    },
    reduce_playback() {
      throw new Error("not used")
    },
    parse_html_document() {
      throw new Error("not used")
    },
  }))

  assert.deepEqual(await core.createPlaybackState(2), {
    status: "idle",
    current_sentence: 0,
    sentence_count: 2,
    generation: 0,
    current_duration_ms: null,
    current_position_ms: 0,
    last_tick_monotonic_ms: null,
    sleep_deadline_unix_ms: null,
    error: null,
  })
})

test("the web boundary sends playback events to the shared reducer", async () => {
  const core = createRustCoreClient(async () => ({
    create_playback_state() {
      throw new Error("not used")
    },
    reduce_playback(policyJSON, stateJSON, eventJSON) {
      if (
        policyJSON !== undefined ||
        stateJSON !==
          '{"status":"idle","current_sentence":0,"sentence_count":2,"generation":0,"current_duration_ms":null,"current_position_ms":0,"last_tick_monotonic_ms":null,"sleep_deadline_unix_ms":null,"error":null}' ||
        eventJSON !== '{"type":"play_requested"}'
      ) {
        throw new Error("unexpected reducer input")
      }
      return JSON.stringify({
        state: {
          status: "loading",
          current_sentence: 0,
          sentence_count: 2,
          generation: 1,
          current_duration_ms: null,
          current_position_ms: 0,
          last_tick_monotonic_ms: null,
          sleep_deadline_unix_ms: null,
          error: null,
        },
        effects: [
          { type: "fetch_audio", sentence: 0, generation: 1, priority: "user" },
        ],
      })
    },
    parse_html_document() {
      throw new Error("not used")
    },
  }))

  assert.deepEqual(
    await core.reducePlayback(
      {
        status: "idle",
        current_sentence: 0,
        sentence_count: 2,
        generation: 0,
        current_duration_ms: null,
        current_position_ms: 0,
        last_tick_monotonic_ms: null,
        sleep_deadline_unix_ms: null,
        error: null,
      },
      { type: "play_requested" }
    ),
    {
      state: {
        status: "loading",
        current_sentence: 0,
        sentence_count: 2,
        generation: 1,
        current_duration_ms: null,
        current_position_ms: 0,
        last_tick_monotonic_ms: null,
        sleep_deadline_unix_ms: null,
        error: null,
      },
      effects: [
        { type: "fetch_audio", sentence: 0, generation: 1, priority: "user" },
      ],
    }
  )
})

test("the web boundary returns the shared document IR for HTML", async () => {
  const core = createRustCoreClient(async () => ({
    create_playback_state() {
      throw new Error("not used")
    },
    reduce_playback() {
      throw new Error("not used")
    },
    parse_html_document(html, optionsJSON) {
      if (
        html !== "<h1>Hello</h1>" ||
        optionsJSON !==
          '{"max_input_bytes":1024,"max_blocks":10,"base_url":"https://readio.example/books/"}'
      ) {
        throw new Error("unexpected document input")
      }
      return JSON.stringify({
        ir_version: 1,
        blocks: [
          {
            id: "block-heading",
            kind: { type: "heading", level: 1 },
            text: "Hello",
          },
        ],
        assets: [],
      })
    },
  }))

  assert.deepEqual(
    await core.parseHTML("<h1>Hello</h1>", {
      max_input_bytes: 1024,
      max_blocks: 10,
      base_url: "https://readio.example/books/",
    }),
    {
      ir_version: 1,
      blocks: [
        {
          id: "block-heading",
          kind: { type: "heading", level: 1 },
          text: "Hello",
        },
      ],
      assets: [],
    }
  )
})

test("the web boundary returns complete Reader IR for HTML", async () => {
  const reader = {
    ir_version: 1,
    document: { ir_version: 1, blocks: [], assets: [] },
    text_index: {
      version: 1,
      document_revision: "reader-revision",
      sentences: [],
    },
    chapters: [],
    navigation: [],
  }
  const core = createRustCoreClient(async () => ({
    create_playback_state() {
      throw new Error("not used")
    },
    reduce_playback() {
      throw new Error("not used")
    },
    parse_html_document() {
      throw new Error("not used")
    },
    parse_html_reader(html, optionsJSON) {
      assert.equal(html, "<p>One.</p>")
      assert.equal(
        optionsJSON,
        '{"max_input_bytes":2048,"max_blocks":20,"base_url":null}'
      )
      return JSON.stringify(reader)
    },
  }))

  assert.deepEqual(
    await core.parseHTMLReader("<p>One.</p>", {
      max_input_bytes: 2048,
      max_blocks: 20,
      base_url: null,
    }),
    reader
  )
})

test("the web boundary returns complete Reader IR for plain text", async () => {
  const reader = {
    ir_version: 1,
    document: { ir_version: 1, blocks: [], assets: [] },
    text_index: {
      version: 1,
      document_revision: "plain-reader-revision",
      sentences: [],
    },
    chapters: [],
    navigation: [],
  }
  const core = createRustCoreClient(async () => ({
    create_playback_state() {
      throw new Error("not used")
    },
    reduce_playback() {
      throw new Error("not used")
    },
    parse_html_document() {
      throw new Error("not used")
    },
    parse_plain_text_reader(text, optionsJSON) {
      assert.equal(text, "One.\n\nTwo.")
      assert.equal(optionsJSON, undefined)
      return JSON.stringify(reader)
    },
  }))

  assert.deepEqual(await core.parsePlainTextReader("One.\n\nTwo."), reader)
})

test("the web boundary returns semantic Reader IR for Markdown", async () => {
  const reader = {
    ir_version: 1,
    document: {
      ir_version: 1,
      blocks: [
        {
          id: "heading",
          kind: { type: "heading", level: 1 },
          text: "Title",
          inline_spans: [],
        },
      ],
      assets: [],
    },
    text_index: {
      version: 1,
      document_revision: "markdown-reader-revision",
      sentences: [],
    },
    chapters: [],
    navigation: [],
  }
  const core = createRustCoreClient(async () => ({
    create_playback_state() {
      throw new Error("not used")
    },
    reduce_playback() {
      throw new Error("not used")
    },
    parse_html_document() {
      throw new Error("not used")
    },
    parse_markdown_reader(markdown, optionsJSON) {
      assert.equal(markdown, "# Title")
      assert.equal(optionsJSON, undefined)
      return JSON.stringify(reader)
    },
  }))

  assert.deepEqual(await core.parseMarkdownReader("# Title"), reader)
})

test("the web boundary delegates PDF bytes to the shared media parser", async () => {
  const core = createRustCoreClient(async () => ({
    create_playback_state() {
      throw new Error("not used")
    },
    reduce_playback() {
      throw new Error("not used")
    },
    parse_html_document() {
      throw new Error("not used")
    },
    parse_pdf(bytes) {
      assert.deepEqual([...bytes], [37, 80, 68, 70])
      return JSON.stringify({
        title: "Shared PDF",
        author: "Rust SDK",
        canonical_text: "Shared PDF text.",
        page_count: 1,
        chapters: [
          {
            title: null,
            page_start: 1,
            page_end: 1,
            document: {
              ir_version: 1,
              blocks: [
                {
                  id: "pdf-block-1",
                  kind: { type: "paragraph" },
                  text: "Shared PDF text.",
                },
              ],
              assets: [],
            },
            block_provenance: [
              {
                block_id: "pdf-block-1",
                page_start: 1,
                page_end: 1,
              },
            ],
          },
        ],
      })
    },
  }))

  assert.deepEqual(await core.parsePDF(new Uint8Array([37, 80, 68, 70])), {
    title: "Shared PDF",
    author: "Rust SDK",
    canonical_text: "Shared PDF text.",
    page_count: 1,
    chapters: [
      {
        title: null,
        page_start: 1,
        page_end: 1,
        document: {
          ir_version: 1,
          blocks: [
            {
              id: "pdf-block-1",
              kind: { type: "paragraph" },
              text: "Shared PDF text.",
            },
          ],
          assets: [],
        },
        block_provenance: [
          {
            block_id: "pdf-block-1",
            page_start: 1,
            page_end: 1,
          },
        ],
      },
    ],
  })
})

test("the web boundary delegates EPUB bytes to the shared spine parser", async () => {
  const core = createRustCoreClient(async () => ({
    create_playback_state() {
      throw new Error("not used")
    },
    reduce_playback() {
      throw new Error("not used")
    },
    parse_html_document() {
      throw new Error("not used")
    },
    parse_epub(bytes) {
      assert.deepEqual([...bytes], [80, 75, 3, 4])
      return JSON.stringify({
        title: "Shared EPUB",
        author: "Rust SDK",
        chapters: [
          {
            id: "chapter-1",
            href: "chapter.xhtml",
            title: "One",
            document: {
              ir_version: 1,
              blocks: [
                {
                  id: "block-1",
                  kind: { type: "heading", level: 1 },
                  text: "One",
                },
              ],
              assets: [],
            },
          },
        ],
      })
    },
  }))

  const publication = await core.parseEPUB(new Uint8Array([80, 75, 3, 4]))
  assert.equal(publication.title, "Shared EPUB")
  assert.equal(publication.chapters[0]?.id, "chapter-1")
  assert.equal(publication.chapters[0]?.document.blocks[0]?.text, "One")
})

test("the web boundary returns raw EPUB asset bytes by stable ID", async () => {
  const core = createRustCoreClient(async () => ({
    create_playback_state() {
      throw new Error("not used")
    },
    reduce_playback() {
      throw new Error("not used")
    },
    parse_html_document() {
      throw new Error("not used")
    },
    extract_epub_asset(bytes, assetId) {
      assert.deepEqual([...bytes], [80, 75, 3, 4])
      assert.equal(assetId, "epub-asset-figure")
      return new Uint8Array([137, 80, 78, 71])
    },
  }))

  assert.deepEqual(
    [
      ...(await core.extractEPUBAsset(
        new Uint8Array([80, 75, 3, 4]),
        "epub-asset-figure"
      )),
    ],
    [137, 80, 78, 71]
  )
})

test("the web boundary delegates import identity and ownership plans to WASM", async () => {
  const core = createRustCoreClient(async () => ({
    create_playback_state() {
      throw new Error("not used")
    },
    reduce_playback() {
      throw new Error("not used")
    },
    parse_html_document() {
      throw new Error("not used")
    },
    content_identity(bytes) {
      assert.deepEqual([...bytes], [1, 2, 3])
      return JSON.stringify({
        algorithm: "blake3-prefix-v1",
        byte_length: 3,
        digest: "digest",
      })
    },
    plan_import(requestJSON) {
      const request = JSON.parse(requestJSON)
      assert.equal(request.ownership, "move")
      assert.equal(request.content_identity.digest, "digest")
      return JSON.stringify({
        outcome: "created",
        record: {
          item_id: "digest",
          content_identity: request.content_identity,
          ownership: "move",
          source_key: "incoming/book.txt",
          storage_key: "library/book.txt",
        },
        effects: [
          {
            type: "write_owned_content",
            source_key: "incoming/book.txt",
            storage_key: "library/book.txt",
          },
          { type: "delete_source_after_commit", source_key: "incoming/book.txt" },
        ],
      })
    },
    plan_forget(recordJSON) {
      const record = JSON.parse(recordJSON)
      return JSON.stringify([
        { type: "delete_owned_content", storage_key: record.storage_key },
      ])
    },
  }))

  const identity = await core.contentIdentity(new Uint8Array([1, 2, 3]))
  const plan = await core.planImport({
    content_identity: identity,
    ownership: "move",
    source_key: "incoming/book.txt",
    owned_storage_key: "library/book.txt",
    existing: null,
  })
  assert.equal(plan.outcome, "created")
  assert.deepEqual(plan.effects.map((effect) => effect.type), [
    "write_owned_content",
    "delete_source_after_commit",
  ])
  assert.deepEqual(await core.planForget(plan.record), [
    { type: "delete_owned_content", storage_key: "library/book.txt" },
  ])
})

test("the web boundary projects a scalar position through WASM", async () => {
  const document = {
    ir_version: 1,
    revision: {
      content_identity: { algorithm: "blake3-prefix-v1", byte_length: 1, digest: "content" },
      ir_version: 1,
      parser_version: "html-ir-1",
      revision_digest: "revision",
      char_count: 5,
    },
    blocks: [],
    assets: [],
  }
  const core = createRustCoreClient(async () => ({
    create_playback_state() {
      throw new Error("not used")
    },
    reduce_playback() {
      throw new Error("not used")
    },
    parse_html_document() {
      throw new Error("not used")
    },
    project_offset(documentJSON, requestJSON) {
      assert.equal(documentJSON, JSON.stringify(document))
      assert.equal(requestJSON, '{"char_offset":2,"affinity":"forward"}')
      return JSON.stringify({
        char_offset: 2,
        block_id: "block-1",
        offset_in_block: 2,
        chapter_id: null,
        sentence_index: null,
        progress_basis_points: 4000,
      })
    },
    restore_position() {
      throw new Error("not used")
    },
  }))

  assert.deepEqual(await core.projectOffset(document, 2, "forward"), {
    char_offset: 2,
    block_id: "block-1",
    offset_in_block: 2,
    chapter_id: null,
    sentence_index: null,
    progress_basis_points: 4000,
  })
})

test("the web boundary resolves a block anchor through WASM", async () => {
  const document = {
    ir_version: 1,
    revision: {
      content_identity: { algorithm: "blake3-prefix-v1", byte_length: 6, digest: "content" },
      ir_version: 1,
      parser_version: "html-ir-1",
      revision_digest: "revision",
      char_count: 5,
    },
    blocks: [
      { id: "block-1", kind: { type: "paragraph" as const }, text: "A🙂" },
      { id: "block-2", kind: { type: "paragraph" as const }, text: "中B" },
    ],
    assets: [],
  }
  const core = createRustCoreClient(async () => ({
    create_playback_state() {
      throw new Error("not used")
    },
    reduce_playback() {
      throw new Error("not used")
    },
    parse_html_document() {
      throw new Error("not used")
    },
    resolve_block_offset(documentJSON, requestJSON) {
      assert.equal(documentJSON, JSON.stringify(document))
      assert.equal(
        requestJSON,
        '{"block_id":"block-2","offset_in_block":1,"affinity":"forward"}'
      )
      return JSON.stringify({
        char_offset: 4,
        block_id: "block-2",
        offset_in_block: 1,
        chapter_id: null,
        sentence_index: null,
        progress_basis_points: 8000,
      })
    },
  }))

  assert.equal(
    (await core.resolveBlockOffset(document, "block-2", 1, "forward")).char_offset,
    4
  )
})

test("the web boundary restores a position without changing its durable coordinate", async () => {
  const document = {
    ir_version: 1,
    revision: {
      content_identity: { algorithm: "blake3-prefix-v1", byte_length: 1, digest: "content" },
      ir_version: 1,
      parser_version: "html-ir-1",
      revision_digest: "revision",
      char_count: 5,
    },
    blocks: [],
    assets: [],
  }
  const position = {
    item_id: "book-1",
    document_revision: "stale",
    char_offset: 999,
    affinity: "backward" as const,
    observed_at_unix_ms: 10,
    session_id: "s",
    mutation_id: "m",
    base_version: 1,
  }
  const core = createRustCoreClient(async () => ({
    create_playback_state() {
      throw new Error("not used")
    },
    reduce_playback() {
      throw new Error("not used")
    },
    parse_html_document() {
      throw new Error("not used")
    },
    project_offset() {
      throw new Error("not used")
    },
    restore_position(documentJSON, positionJSON) {
      assert.equal(documentJSON, JSON.stringify(document))
      assert.equal(positionJSON, JSON.stringify(position))
      return JSON.stringify({
        char_offset: 5,
        affinity: "backward",
        revision_changed: true,
        projection: {
          char_offset: 5,
          block_id: "block-1",
          offset_in_block: 1,
          chapter_id: null,
          sentence_index: null,
          progress_basis_points: 10000,
        },
      })
    },
  }))

  assert.deepEqual(await core.restorePosition(position, document), {
    char_offset: 5,
    affinity: "backward",
    revision_changed: true,
    projection: {
      char_offset: 5,
      block_id: "block-1",
      offset_in_block: 1,
      chapter_id: null,
      sentence_index: null,
      progress_basis_points: 10000,
    },
  })
})

test("the web boundary exposes Rust locators and renderer highlight ranges", async () => {
  const document = {
    ir_version: 1,
    revision: {
      content_identity: { algorithm: "blake3-prefix-v1", byte_length: 1, digest: "content" },
      ir_version: 1,
      parser_version: "html-ir-3",
      revision_digest: "revision",
      char_count: 39,
    },
    blocks: [
      { id: "first", kind: { type: "paragraph" as const }, text: "First sentence." },
      { id: "second", kind: { type: "paragraph" as const }, text: "Second bright sentence." },
    ],
    assets: [],
  }
  const textIndex = {
    version: 1,
    document_revision: "revision",
    sentences: [],
  }
  const core = createRustCoreClient(async () => ({
    create_playback_state() { throw new Error("not used") },
    reduce_playback() { throw new Error("not used") },
    parse_html_document() { throw new Error("not used") },
    create_text_locator(documentJSON, requestJSON) {
      assert.equal(documentJSON, JSON.stringify(document))
      assert.equal(requestJSON, '{"char_offset":23,"affinity":"forward"}')
      return JSON.stringify({
        version: 1,
        document_revision: "revision",
        char_offset: 23,
        block_id: "second",
        offset_in_block: 7,
        affinity: "forward",
        quote: { prefix: "", exact: "bright sentence.", suffix: "", offset_in_exact: 0 },
      })
    },
    restore_text_locator(documentJSON, locatorJSON) {
      assert.equal(documentJSON, JSON.stringify(document))
      assert.equal(JSON.parse(locatorJSON).block_id, "second")
      return JSON.stringify({
        kind: "exact_revision",
        projection: {
          char_offset: 23,
          block_id: "second",
          offset_in_block: 7,
          chapter_id: null,
          sentence_index: null,
          progress_basis_points: 5897,
        },
      })
    },
    index_document_text(documentJSON) {
      assert.equal(documentJSON, JSON.stringify(document))
      return JSON.stringify(textIndex)
    },
    resolve_highlight(documentJSON, indexJSON, cursorJSON) {
      assert.equal(documentJSON, JSON.stringify(document))
      assert.equal(indexJSON, JSON.stringify(textIndex))
      assert.deepEqual(JSON.parse(cursorJSON), {
        document_revision: "revision",
        sentence_index: 1,
        word_index: 1,
      })
      return JSON.stringify({
        sentence_id: "sentence-2",
        sentence_index: 1,
        block_id: "second",
        sentence_range: [0, 23],
        word_range: [7, 13],
        sentence_char_range: [16, 39],
        word_char_range: [23, 29],
      })
    },
  }))

  const locator = await core.createTextLocator(document, 23, "forward")
  assert.equal((await core.restoreTextLocator(locator, document)).kind, "exact_revision")
  const index = await core.indexDocumentText(document)
  assert.deepEqual(
    await core.resolveHighlight(document, index, {
      document_revision: "revision",
      sentence_index: 1,
      word_index: 1,
    }),
    {
      sentence_id: "sentence-2",
      sentence_index: 1,
      block_id: "second",
      sentence_range: [0, 23],
      word_range: [7, 13],
      sentence_char_range: [16, 39],
      word_char_range: [23, 29],
    }
  )
})

test("the web boundary returns folded search hits from WASM", async () => {
  const document = {
    ir_version: 1,
    revision: {
      content_identity: { algorithm: "blake3-prefix-v1", byte_length: 1, digest: "content" },
      ir_version: 1,
      parser_version: "html-ir-1",
      revision_digest: "revision",
      char_count: 14,
    },
    blocks: [],
    assets: [],
  }
  const request = {
    document_revision: "revision",
    query: "city",
    cursor: null,
    limit: 10,
  }
  const core = createRustCoreClient(async () => ({
    create_playback_state() { throw new Error("not used") },
    reduce_playback() { throw new Error("not used") },
    parse_html_document() { throw new Error("not used") },
    search_document(documentJSON, requestJSON) {
      assert.equal(documentJSON, JSON.stringify(document))
      assert.equal(requestJSON, JSON.stringify(request))
      return JSON.stringify({
        folded_query: "city",
        total_occurrences: 2,
        total_blocks: 1,
        hits: [{
          hit_id: "block:0:4",
          char_range: [0, 4],
          block_id: "block",
          snippet: "ＣＩＴＹ and city",
          snippet_match_range: [0, 4],
        }],
        next_cursor: null,
      })
    },
  }))

  const result = await core.searchDocument(document, request)
  assert.equal(result.total_occurrences, 2)
  assert.deepEqual(result.hits[0]?.char_range, [0, 4])
})

test("the web boundary applies bookmark semantics through WASM", async () => {
  const document = { ir_version: 1, blocks: [], assets: [] }
  const request = {
    bookmark_id: "bookmark-1",
    item_id: "book-1",
    char_offset: 6,
    affinity: "forward" as const,
    note: "saved",
    created_at_unix_ms: 10,
    updated_at_unix_ms: 10,
    base_version: 0,
  }
  const core = createRustCoreClient(async () => ({
    create_playback_state() { throw new Error("not used") },
    reduce_playback() { throw new Error("not used") },
    parse_html_document() { throw new Error("not used") },
    upsert_bookmark(documentJSON, bookmarksJSON, requestJSON) {
      assert.equal(documentJSON, JSON.stringify(document))
      assert.equal(bookmarksJSON, "[]")
      assert.equal(requestJSON, JSON.stringify(request))
      return JSON.stringify({
        created: true,
        bookmark: {
          id: "bookmark-1",
          item_id: "book-1",
          char_offset: 6,
          affinity: "forward",
          note: "saved",
          created_at_unix_ms: 10,
          updated_at_unix_ms: 10,
          version: 1,
        },
      })
    },
  }))

  const result = await core.upsertBookmark(document, [], request)
  assert.equal(result.created, true)
  assert.equal(result.bookmark.version, 1)
})
