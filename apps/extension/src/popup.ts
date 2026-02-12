import { buildSynthesizeUrl, createSynthesizePayload, type SynthesizeResponse, type TtsProvider } from './lib/tts-client'
import { pickPlayableText, trimForTts } from './lib/selection'
import { buildCreateLibraryItemUrl, buildLibraryItemPayload } from './lib/library-client'

type ExtractResult = {
  selected: string
  article: string
  pageTitle: string
  pageUrl: string
}

type PopupState = {
  apiBase: string
  provider: TtsProvider
  speed: number
}

const DEFAULT_STATE: PopupState = {
  apiBase: 'http://127.0.0.1:8000',
  provider: 'auto',
  speed: 1,
}

function byId<T extends HTMLElement>(id: string): T {
  const node = document.getElementById(id)
  if (!node) {
    throw new Error(`Missing element: ${id}`)
  }
  return node as T
}

const statusEl = byId<HTMLParagraphElement>('status')
const sourceEl = byId<HTMLTextAreaElement>('sourceText')
const apiBaseEl = byId<HTMLInputElement>('apiBase')
const providerEl = byId<HTMLSelectElement>('provider')
const speedEl = byId<HTMLInputElement>('speed')
const speedValueEl = byId<HTMLSpanElement>('speedValue')
const captureBtn = byId<HTMLButtonElement>('captureSelection')
const synthBtn = byId<HTMLButtonElement>('synthesize')
const saveBtn = byId<HTMLButtonElement>('saveLibrary')
const stopBtn = byId<HTMLButtonElement>('stopAudio')
const audioEl = byId<HTMLAudioElement>('player')
let lastCaptureMeta: { pageTitle: string; pageUrl: string } | null = null

function setStatus(message: string, isError: boolean = false): void {
  statusEl.textContent = message
  statusEl.dataset.variant = isError ? 'error' : 'info'
}

function setBusy(isBusy: boolean): void {
  captureBtn.disabled = isBusy
  synthBtn.disabled = isBusy
  saveBtn.disabled = isBusy
}

async function loadState(): Promise<PopupState> {
  const data = await chrome.storage.sync.get(['apiBase', 'provider', 'speed'])
  return {
    apiBase: typeof data.apiBase === 'string' ? data.apiBase : DEFAULT_STATE.apiBase,
    provider: (data.provider as TtsProvider) || DEFAULT_STATE.provider,
    speed: typeof data.speed === 'number' ? data.speed : DEFAULT_STATE.speed,
  }
}

async function saveState(state: PopupState): Promise<void> {
  await chrome.storage.sync.set(state)
}

async function extractFromActiveTab(): Promise<ExtractResult> {
  const tabs = await chrome.tabs.query({ active: true, currentWindow: true })
  const tab = tabs[0]
  if (!tab?.id) {
    throw new Error('No active tab available')
  }

  const injected = await chrome.scripting.executeScript({
    target: { tabId: tab.id },
    func: () => {
      const selection = window.getSelection()?.toString().trim() ?? ''
      const paragraphText = Array.from(document.querySelectorAll('article p, main p, p'))
        .slice(0, 24)
        .map((node) => node.textContent?.trim() ?? '')
        .filter(Boolean)
        .join(' ')

      return {
        selected: selection,
        article: paragraphText,
        pageTitle: document.title,
        pageUrl: location.href,
      }
    },
  })

  const payload = injected[0]?.result
  if (!payload) {
    throw new Error('Failed to extract page text')
  }
  return payload
}

function hydrateForm(state: PopupState): void {
  apiBaseEl.value = state.apiBase
  providerEl.value = state.provider
  speedEl.value = String(state.speed)
  speedValueEl.textContent = `${state.speed.toFixed(2)}x`
}

function readStateFromForm(): PopupState {
  return {
    apiBase: apiBaseEl.value.trim() || DEFAULT_STATE.apiBase,
    provider: (providerEl.value as TtsProvider) || 'auto',
    speed: Number(speedEl.value),
  }
}

async function synthesizeCurrentText(): Promise<void> {
  const text = trimForTts(sourceEl.value)
  if (!text) {
    setStatus('Capture text first.', true)
    return
  }

  const state = readStateFromForm()
  await saveState(state)
  const url = buildSynthesizeUrl(state.apiBase, state.provider)
  const payload = createSynthesizePayload(text, state.speed)

  setBusy(true)
  setStatus('Synthesizing...')

  try {
    const response = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    })

    if (!response.ok) {
      const detail = await response.text()
      throw new Error(`API ${response.status}: ${detail}`)
    }

    const data = (await response.json()) as SynthesizeResponse
    audioEl.src = `data:audio/mpeg;base64,${data.audio_base64}`
    await audioEl.play()
    setStatus(`Playing via ${data.provider}`)
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Synthesis failed'
    setStatus(message, true)
  } finally {
    setBusy(false)
  }
}

async function captureSelection(): Promise<void> {
  setBusy(true)
  setStatus('Capturing tab text...')

  try {
    const extract = await extractFromActiveTab()
    const text = trimForTts(pickPlayableText(extract.selected, extract.article))
    sourceEl.value = text
    lastCaptureMeta = { pageTitle: extract.pageTitle, pageUrl: extract.pageUrl }

    if (!text) {
      setStatus('No readable text found on this page.', true)
      return
    }

    setStatus(`Captured ${text.length} chars from ${extract.pageTitle || 'page'}.`)
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Capture failed'
    setStatus(message, true)
  } finally {
    setBusy(false)
  }
}

async function saveToLibrary(): Promise<void> {
  const text = trimForTts(sourceEl.value)
  if (!text) {
    setStatus('Capture text first.', true)
    return
  }

  const state = readStateFromForm()
  await saveState(state)
  const sourceUrl = lastCaptureMeta?.pageUrl || 'chrome-extension://capture'
  const title = (lastCaptureMeta?.pageTitle || 'Captured Page').trim() || 'Captured Page'

  setBusy(true)
  setStatus('Saving to library...')

  try {
    const response = await fetch(buildCreateLibraryItemUrl(state.apiBase), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(
        buildLibraryItemPayload({
          title,
          content: text,
          sourceUrl,
        }),
      ),
    })

    if (!response.ok) {
      const detail = await response.text()
      throw new Error(`API ${response.status}: ${detail}`)
    }

    setStatus('Saved to library')
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Save failed'
    setStatus(message, true)
  } finally {
    setBusy(false)
  }
}

async function bootstrap(): Promise<void> {
  const stored = await loadState()
  hydrateForm(stored)
  setStatus('Ready')

  speedEl.addEventListener('input', () => {
    speedValueEl.textContent = `${Number(speedEl.value).toFixed(2)}x`
  })

  apiBaseEl.addEventListener('change', () => {
    void saveState(readStateFromForm())
  })

  providerEl.addEventListener('change', () => {
    void saveState(readStateFromForm())
  })

  speedEl.addEventListener('change', () => {
    void saveState(readStateFromForm())
  })

  captureBtn.addEventListener('click', () => {
    void captureSelection()
  })

  synthBtn.addEventListener('click', () => {
    void synthesizeCurrentText()
  })

  saveBtn.addEventListener('click', () => {
    void saveToLibrary()
  })

  stopBtn.addEventListener('click', () => {
    audioEl.pause()
    audioEl.currentTime = 0
    setStatus('Playback stopped')
  })
}

void bootstrap()
