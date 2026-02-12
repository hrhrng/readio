# MiniMax Cloud Setup (Recommended Fast Path)

Run Readio with MiniMax as the cloud fallback provider.

## 1) Install

```bash
git clone <your-repo-url>
cd readio
make install
```

## 2) Configure API credentials

```bash
cp apps/api/.env.example apps/api/.env
```

Set these in `apps/api/.env`:

- `MINIMAX_API_KEY=...`
- `MINIMAX_VOICE_ID=...` (default in example is `English_expressive_narrator`)

Optional:

- `MINIMAX_GROUP_ID=...` (only if your account requires GroupId)
- `MINIMAX_API_URL=https://api.minimax.chat/v1/t2a_v2` (full endpoint override)
- `MINIMAX_MODEL_ID=speech-2.6-hd`

## 3) Start web + API

```bash
make dev-minimax
```

This runs:

- Web: `http://127.0.0.1:3000`
- API: `http://127.0.0.1:8000`

Fallback order is set to:

- `edge -> minimax -> elevenlabs`

## 4) Verify provider status

```bash
curl http://127.0.0.1:8000/api/providers
```

You should see `minimax` with `available: true`.

## 5) Quick synth test

```bash
curl -X POST "http://127.0.0.1:8000/api/tts/synthesize?provider=minimax" \
  -H "content-type: application/json" \
  -d '{"text":"Hello from Readio MiniMax mode."}'
```

## 6) Streaming test (MiniMax upstream audio stream)

```bash
curl -N -X POST "http://127.0.0.1:8000/api/tts/stream?provider=minimax" \
  -H "content-type: application/json" \
  -d '{"text":"This is a streaming test from Readio and MiniMax."}'
```

`chunk` events carry `audio_base64` as audio chunks from MiniMax streaming.
