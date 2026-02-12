import base64

from fastapi.testclient import TestClient

import app.api.routes as routes
from app.main import app
from app.tts.models import TTSResult


client = TestClient(app)


class DummyRouter:
    def __init__(self):
        self.called = None

    async def synthesize_with_fallback(self, request):
        self.called = ('fallback', request)
        return TTSResult(
            provider='edge',
            audio_bytes=b'hello-audio',
            duration_ms=1200,
            sample_rate=24000,
            trace_id='trace-fallback',
        )

    async def synthesize_with_provider(self, provider, request):
        self.called = ('provider', provider, request)
        return TTSResult(
            provider=provider,
            audio_bytes=b'provider-audio',
            duration_ms=900,
            sample_rate=22050,
            trace_id='trace-provider',
        )


class ErrorRouter:
    async def synthesize_with_fallback(self, request):
        raise ValueError('bad request payload')

    async def synthesize_with_provider(self, provider, request):
        raise ValueError('bad provider')


def test_synthesize_without_provider_uses_fallback(monkeypatch):
    dummy = DummyRouter()
    monkeypatch.setattr(routes, 'tts_router', dummy)

    resp = client.post('/api/tts/synthesize', json={'text': 'hello world'})

    assert resp.status_code == 200
    data = resp.json()
    assert data['provider'] == 'edge'
    assert data['audio_base64'] == base64.b64encode(b'hello-audio').decode('utf-8')
    assert data['trace_id'] == 'trace-fallback'
    assert dummy.called[0] == 'fallback'


def test_synthesize_with_provider_uses_forced_provider(monkeypatch):
    dummy = DummyRouter()
    monkeypatch.setattr(routes, 'tts_router', dummy)

    resp = client.post('/api/tts/synthesize?provider=elevenlabs', json={'text': 'force provider'})

    assert resp.status_code == 200
    data = resp.json()
    assert data['provider'] == 'elevenlabs'
    assert data['audio_base64'] == base64.b64encode(b'provider-audio').decode('utf-8')
    assert dummy.called[0] == 'provider'
    assert dummy.called[1] == 'elevenlabs'


def test_synthesize_maps_value_error_to_400(monkeypatch):
    monkeypatch.setattr(routes, 'tts_router', ErrorRouter())

    resp = client.post('/api/tts/synthesize', json={'text': 'x'})

    assert resp.status_code == 400
    assert 'bad request payload' in resp.json()['detail']
