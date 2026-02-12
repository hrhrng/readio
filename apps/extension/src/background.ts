chrome.runtime.onInstalled.addListener(() => {
  chrome.storage.sync.get(['apiBase', 'provider', 'speed'], (stored) => {
    const defaults: Record<string, string | number> = {}
    if (!stored.apiBase) defaults.apiBase = 'http://localhost:8000'
    if (!stored.provider) defaults.provider = 'auto'
    if (!stored.speed) defaults.speed = 1

    if (Object.keys(defaults).length > 0) {
      chrome.storage.sync.set(defaults)
    }
  })
})
