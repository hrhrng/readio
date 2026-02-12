import base64

from fastapi.testclient import TestClient

import app.api.routes as routes
from app.main import app
from app.tts.models import TTSResult
from app.tts.providers.minimax import MiniMaxStreamChunk


client = TestClient(app)


class DummyStreamRouter:
    def __init__(self) -> None:
        self.calls: list[tuple[str, str]] = []

    async def synthesize_with_fallback(self, request):
        self.calls.append(('fallback', request.text))
        return TTSResult(
            provider='edge',
            audio_bytes=f'audio:{request.text}'.encode('utf-8'),
            duration_ms=1200,
            sample_rate=24000,
            trace_id='trace-fallback',
        )

    async def synthesize_with_provider(self, provider, request):
        self.calls.append((provider, request.text))
        return TTSResult(
            provider=provider,
            audio_bytes=f'audio:{request.text}'.encode('utf-8'),
            duration_ms=900,
            sample_rate=22050,
            trace_id='trace-provider',
        )


class DummyMiniMaxStreamProvider:
    def __init__(self) -> None:
        self.calls: list[str] = []

    async def stream_synthesize(self, request):
        self.calls.append(request.text)
        yield MiniMaxStreamChunk(audio_bytes=b'mini-1', trace_id='trace-minimax', status=1)
        yield MiniMaxStreamChunk(audio_bytes=b'mini-2', trace_id='trace-minimax', status=2)


class DummyRouterWithMiniMaxStream(DummyStreamRouter):
    def __init__(self) -> None:
        super().__init__()
        self.minimax_provider = DummyMiniMaxStreamProvider()
        self.providers = {'minimax': self.minimax_provider}


def test_tts_stream_emits_sse_events(monkeypatch):
    dummy = DummyStreamRouter()
    monkeypatch.setattr(routes, 'tts_router', dummy)

    resp = client.post('/api/tts/stream', json={'text': 'Hello world. Bye now!'})

    assert resp.status_code == 200
    assert resp.headers['content-type'].startswith('text/event-stream')

    body = resp.text
    assert 'event: start' in body
    assert body.count('event: chunk') == 2
    assert 'event: end' in body

    first_chunk_b64 = base64.b64encode(b'audio:Hello world.').decode('utf-8')
    assert first_chunk_b64 in body


def test_tts_stream_forced_provider_uses_selected_provider(monkeypatch):
    dummy = DummyStreamRouter()
    monkeypatch.setattr(routes, 'tts_router', dummy)

    resp = client.post('/api/tts/stream?provider=minimax', json={'text': 'A. B.'})

    assert resp.status_code == 200
    assert dummy.calls == [('minimax', 'A.'), ('minimax', 'B.')]


def test_tts_stream_minimax_uses_upstream_provider_stream_when_available(monkeypatch):
    dummy = DummyRouterWithMiniMaxStream()
    monkeypatch.setattr(routes, 'tts_router', dummy)

    resp = client.post('/api/tts/stream?provider=minimax', json={'text': 'A. B.'})

    assert resp.status_code == 200
    assert dummy.calls == []
    assert dummy.minimax_provider.calls == ['A. B.']

    body = resp.text
    assert body.count('event: chunk') == 2
    assert base64.b64encode(b'mini-1').decode('utf-8') in body
    assert '"stream_status": 2' in body
