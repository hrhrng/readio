# Readio Monorepo

Open-source Speechify-like reading stack with:

- `apps/web` (Next.js)
- `apps/api` (FastAPI)
- `apps/extension` (Chrome MV3)

## Quick Start (Edge + Cloud Fallback)

1. Install monorepo dependencies:

```bash
make install
```

2. Create `apps/api/.env` from example:

```bash
cp apps/api/.env.example apps/api/.env
```

Set at least:

- `MINIMAX_API_KEY=...`
- `MINIMAX_VOICE_ID=...` (default is `English_expressive_narrator`)

Optional:

- `MINIMAX_GROUP_ID=...` (if your account requires GroupId)
- `MINIMAX_API_URL=https://api.minimax.chat/v1/t2a_v2` (full endpoint override)

3. Start web + API with Edge-first fallback:

```bash
make dev-minimax
```

This starts:

- API on `http://127.0.0.1:8000` (fallback order: `edge -> minimax -> elevenlabs`)
- Web on `http://127.0.0.1:3000`

4. Open Web app:

- [http://127.0.0.1:3000](http://127.0.0.1:3000)

## Detailed Setup

MiniMax cloud mode:

- `docs/minimax-cloud-setup.md`

Edge TTS works without API keys after `make install` and is the default multilingual path.

## Streaming TTS

API supports `text/event-stream` synthesis:

- `POST /api/tts/stream`
- query:
  - `provider` (optional): `edge | minimax | elevenlabs`
  - `max_chars` (optional, default `220`)

Response is `text/event-stream` with events:

- `start`
- `chunk` (contains `audio_base64`, `text`, `provider`, `trace_id`)
- `end`
- `error`

Behavior:

- `provider=minimax`: uses MiniMax upstream streaming when available (audio chunks from provider).
- other providers (or auto fallback): uses sentence split + per-chunk synth.
- Web app currently uses this stream endpoint when you explicitly select `minimax`.

Note:

- MiniMax stream here is **audio-chunk streaming**, not text-token streaming.
