import types

import pytest

from app.tts.models import TTSRequest
from app.tts.providers.edge import EdgeTTSProvider


class _FakeCommunicate:
    def __init__(self, text: str, voice: str, rate: str):
        self.text = text
        self.voice = voice
        self.rate = rate

    async def save(self, output_path: str) -> None:
        with open(output_path, 'wb') as fh:
            fh.write(b'edge-mp3-audio')


@pytest.mark.asyncio
async def test_edge_provider_synthesize_uses_edge_tts_module(monkeypatch):
    provider = EdgeTTSProvider()

    fake_module = types.SimpleNamespace(Communicate=_FakeCommunicate)
    monkeypatch.setattr('app.tts.providers.edge._load_edge_tts_module', lambda: fake_module)

    result = await provider.synthesize(
        TTSRequest(text='hello multilingual world', speed=1.2, voice='en-US-AriaNeural')
    )

    assert result.provider == 'edge'
    assert result.audio_bytes == b'edge-mp3-audio'


def test_edge_provider_is_unavailable_without_package(monkeypatch):
    provider = EdgeTTSProvider()
    monkeypatch.setattr('app.tts.providers.edge._load_edge_tts_module', lambda: None)
    assert provider.is_available() is False


@pytest.mark.asyncio
async def test_edge_provider_rejects_non_mp3(monkeypatch):
    provider = EdgeTTSProvider()
    fake_module = types.SimpleNamespace(Communicate=_FakeCommunicate)
    monkeypatch.setattr('app.tts.providers.edge._load_edge_tts_module', lambda: fake_module)

    with pytest.raises(RuntimeError) as exc:
        await provider.synthesize(TTSRequest(text='hello', format='wav'))

    assert 'only supports mp3 output' in str(exc.value)
