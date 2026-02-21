# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Install all dependencies (npm workspaces + Python API via uv)
make install

# Start web (port 3000) + API (port 8000) together
make dev              # edge TTS (no API keys needed)
make dev-minimax      # minimax fallback order

# Start individual services
make dev-web          # Next.js only
make dev-backend      # FastAPI only
make dev-extension    # Chrome extension (esbuild watch)

# Tests
make test             # run all (api + web + extension)
make test-api         # pytest: cd apps/api && uv run --all-groups pytest tests
make test-web         # npm run test --workspace @readio/web
make test-extension   # vitest: npm run test --workspace @readio/extension

# Run a single API test file
cd apps/api && uv run --all-groups pytest tests/test_health.py -v

# Lint (web only)
npm run lint --workspace @readio/web
```

## Architecture

Monorepo with three apps sharing no code packages — they communicate via HTTP.

### `apps/api` — FastAPI backend (Python 3.11+, managed by uv)

- **Entry**: `app/main.py` → mounts single `APIRouter` from `app/api/routes.py`
- **TTS pipeline**: Provider pattern with fallback chain
  - `app/tts/base.py` — `TTSProvider` Protocol (id, is_available, synthesize)
  - `app/tts/providers/` — edge, minimax, elevenlabs implementations
  - `app/tts/router.py` — `TTSRouter` tries providers in `TTS_FALLBACK_ORDER` env var order
  - `app/tts/jobs.py` — `TTSJobManager` async priority queue with LRU result cache; web frontend polls job status rather than using the stream endpoint
- **Library**: `app/library/` — SQLite-backed document store (`store.py`), file import from URL/upload (`importers.py`), file storage (`files.py`)
- **Config**: `app/core/settings.py` — dataclass reading from `apps/api/.env` (and repo-root `.env`). All TTS provider keys and tuning knobs live here.

### `apps/web` — Next.js 15 frontend (React 19, Tailwind v4, App Router)

- **API proxy**: `next.config.ts` rewrites `/api/*` → `http://127.0.0.1:8000/api/*` (except file uploads which go direct to avoid Next.js body size limit)
- **Routes**: `app/page.tsx` (home), `app/library/` (browse/filter), `app/player/[id]/` (reader + TTS playback), `app/search/` (search)
- **TTS playback**: `lib/use-tts-player.ts` — hook that creates TTS jobs via the jobs API, polls for completion, manages an `AudioCache`, and prefetches upcoming sentences
- **Reader components**: `components/reader/` — content-router dispatches to epub-reader, pdf-reader, or plain-text-reader based on item type; player-bar provides playback controls
- **Shared lib**: `lib/api.ts` (API client), `lib/types.ts`, `lib/sentences.ts` (text splitting), `lib/hooks.ts`

### `apps/extension` — Chrome MV3 extension

- Built with esbuild (`scripts/build.mjs`), tested with vitest
- `src/background.ts` — sets default storage values on install
- `src/popup.ts` — popup UI for TTS controls
- `src/lib/` — `tts-client.ts`, `library-client.ts`, `selection.ts`

## Development Notes

- Do NOT run `npm run build` or `make build` during development — `make dev` starts dev servers with hot module replacement (HMR), code changes are reflected automatically without rebuilding.

## Code Style

- Python: single-quote strings, type hints everywhere, `noqa: BLE001` on broad exception catches
- TypeScript: double-quote strings (web), no semicolons (extension)
- Write rich comments explaining *why*, not *what*
