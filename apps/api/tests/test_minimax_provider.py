import base64

import pytest
import httpx

from app.core.settings import settings
from app.tts.models import TTSRequest
from app.tts.providers.minimax import decode_audio_payload
from app.tts.providers.minimax import MiniMaxProvider
from app.tts.providers.minimax import resolve_minimax_endpoint


def test_decode_audio_payload_accepts_hex_string():
    assert decode_audio_payload('68656c6c6f') == b'hello'


def test_decode_audio_payload_accepts_base64_string():
    encoded = base64.b64encode(b'hello').decode('utf-8')
    assert decode_audio_payload(encoded) == b'hello'


def test_decode_audio_payload_raises_for_invalid_string():
    with pytest.raises(RuntimeError):
        decode_audio_payload('not-valid-@@@')


def test_resolve_minimax_endpoint_uses_explicit_api_url(monkeypatch):
    monkeypatch.setattr(settings, 'minimax_api_url', 'https://api.minimax.chat/v1/t2a_v2')
    monkeypatch.setattr(settings, 'minimax_group_id', 'g-1')
    monkeypatch.setattr(settings, 'minimax_base_url', 'https://api.minimax.io')
    assert resolve_minimax_endpoint() == 'https://api.minimax.chat/v1/t2a_v2?GroupId=g-1'


def test_resolve_minimax_endpoint_appends_api_path_for_base_url(monkeypatch):
    monkeypatch.setattr(settings, 'minimax_api_url', '')
    monkeypatch.setattr(settings, 'minimax_group_id', '')
    monkeypatch.setattr(settings, 'minimax_base_url', 'https://api.minimax.io')
    assert resolve_minimax_endpoint() == 'https://api.minimax.io/v1/t2a_v2'


class _FakeAsyncClient:
    def __init__(self, *, response: httpx.Response):
        self._response = response
        self.last_url = ''
        self.last_json = None

    async def __aenter__(self):
        return self

    async def __aexit__(self, exc_type, exc, tb):
        return False

    async def post(self, url, headers, json):  # noqa: ANN001
        _ = headers
        self.last_url = url
        self.last_json = json
        return self._response


class _FakeStreamResponse:
    def __init__(self, *, lines: list[str], request_url: str, status_code: int = 200):
        self._lines = lines
        request = httpx.Request('POST', request_url)
        self._response = httpx.Response(status_code, request=request)

    async def __aenter__(self):
        return self

    async def __aexit__(self, exc_type, exc, tb):
        return False

    def raise_for_status(self) -> None:
        self._response.raise_for_status()

    async def aiter_lines(self):
        for line in self._lines:
            yield line


class _FakeStreamingAsyncClient:
    def __init__(self, *, lines: list[str], status_code: int = 200):
        self._lines = lines
        self._status_code = status_code
        self.last_url = ''
        self.last_json = None

    async def __aenter__(self):
        return self

    async def __aexit__(self, exc_type, exc, tb):
        return False

    def stream(self, method, url, headers, json):  # noqa: ANN001
        _ = method, headers
        self.last_url = url
        self.last_json = json
        return _FakeStreamResponse(lines=self._lines, request_url=url, status_code=self._status_code)


@pytest.mark.asyncio
async def test_minimax_provider_allows_missing_group_id_and_uses_default_voice(monkeypatch):
    monkeypatch.setattr(settings, 'minimax_api_key', 'test-key')
    monkeypatch.setattr(settings, 'minimax_group_id', '')
    monkeypatch.setattr(settings, 'minimax_api_url', 'https://api.minimax.chat/v1/t2a_v2')
    monkeypatch.setattr(settings, 'minimax_base_url', 'https://api.minimax.io')
    monkeypatch.setattr(settings, 'minimax_model_id', 'speech-2.6-hd')
    monkeypatch.setattr(settings, 'minimax_voice_id', '')
    monkeypatch.setattr(settings, 'tts_timeout_seconds', 20.0)

    request = httpx.Request('POST', 'https://api.minimax.chat/v1/t2a_v2')
    response = httpx.Response(
        200,
        request=request,
        json={
            'trace_id': 'trace-1',
            'data': {
                'audio': '68656c6c6f',
                'audio_length_ms': 10,
                'sample_rate': 32000,
            },
        },
    )
    fake_client = _FakeAsyncClient(response=response)
    monkeypatch.setattr(httpx, 'AsyncClient', lambda timeout: fake_client)  # noqa: ARG005

    provider = MiniMaxProvider()
    result = await provider.synthesize(TTSRequest(text='hello'))

    assert result.provider == 'minimax'
    assert result.audio_bytes == b'hello'
    assert fake_client.last_url == 'https://api.minimax.chat/v1/t2a_v2'
    assert fake_client.last_json is not None
    assert fake_client.last_json['voice_setting']['voice_id'] == 'English_expressive_narrator'


@pytest.mark.asyncio
async def test_minimax_provider_surfaces_api_error(monkeypatch):
    monkeypatch.setattr(settings, 'minimax_api_key', 'test-key')
    monkeypatch.setattr(settings, 'minimax_group_id', '')
    monkeypatch.setattr(settings, 'minimax_api_url', 'https://api.minimax.chat/v1/t2a_v2')
    monkeypatch.setattr(settings, 'minimax_base_url', 'https://api.minimax.io')
    monkeypatch.setattr(settings, 'minimax_model_id', 'speech-2.6-hd')
    monkeypatch.setattr(settings, 'minimax_voice_id', 'English_expressive_narrator')
    monkeypatch.setattr(settings, 'tts_timeout_seconds', 20.0)

    request = httpx.Request('POST', 'https://api.minimax.chat/v1/t2a_v2')
    response = httpx.Response(
        200,
        request=request,
        json={
            'base_resp': {
                'status_code': 1004,
                'status_msg': 'invalid voice id',
            }
        },
    )
    fake_client = _FakeAsyncClient(response=response)
    monkeypatch.setattr(httpx, 'AsyncClient', lambda timeout: fake_client)  # noqa: ARG005

    provider = MiniMaxProvider()
    with pytest.raises(RuntimeError) as exc:
        await provider.synthesize(TTSRequest(text='hello'))

    assert 'MiniMax API error 1004' in str(exc.value)


@pytest.mark.asyncio
async def test_minimax_provider_stream_synthesize_decodes_sse_chunks(monkeypatch):
    monkeypatch.setattr(settings, 'minimax_api_key', 'test-key')
    monkeypatch.setattr(settings, 'minimax_group_id', '')
    monkeypatch.setattr(settings, 'minimax_api_url', 'https://api.minimax.chat/v1/t2a_v2')
    monkeypatch.setattr(settings, 'minimax_base_url', 'https://api.minimax.io')
    monkeypatch.setattr(settings, 'minimax_model_id', 'speech-2.6-hd')
    monkeypatch.setattr(settings, 'minimax_voice_id', 'English_expressive_narrator')
    monkeypatch.setattr(settings, 'tts_timeout_seconds', 20.0)

    fake_client = _FakeStreamingAsyncClient(
        lines=[
            'event: message',
            'data: {"trace_id":"trace-stream","data":{"audio":"68656c","status":1}}',
            '',
            'data: {"trace_id":"trace-stream","data":{"audio":"6c6f","status":2}}',
            '',
        ]
    )
    monkeypatch.setattr(httpx, 'AsyncClient', lambda timeout: fake_client)  # noqa: ARG005

    provider = MiniMaxProvider()
    chunks = [chunk async for chunk in provider.stream_synthesize(TTSRequest(text='hello'))]

    assert b''.join(chunk.audio_bytes for chunk in chunks) == b'hello'
    assert chunks[-1].status == 2
    assert chunks[-1].trace_id == 'trace-stream'
    assert fake_client.last_url == 'https://api.minimax.chat/v1/t2a_v2'
    assert fake_client.last_json is not None
    assert fake_client.last_json['stream'] is True
    assert fake_client.last_json['output_format'] == 'hex'


@pytest.mark.asyncio
async def test_minimax_provider_stream_synthesize_surfaces_api_error(monkeypatch):
    monkeypatch.setattr(settings, 'minimax_api_key', 'test-key')
    monkeypatch.setattr(settings, 'minimax_group_id', '')
    monkeypatch.setattr(settings, 'minimax_api_url', 'https://api.minimax.chat/v1/t2a_v2')
    monkeypatch.setattr(settings, 'minimax_base_url', 'https://api.minimax.io')
    monkeypatch.setattr(settings, 'minimax_model_id', 'speech-2.6-hd')
    monkeypatch.setattr(settings, 'minimax_voice_id', 'English_expressive_narrator')
    monkeypatch.setattr(settings, 'tts_timeout_seconds', 20.0)

    fake_client = _FakeStreamingAsyncClient(
        lines=[
            'data: {"base_resp":{"status_code":1004,"status_msg":"invalid voice id"}}',
            '',
        ]
    )
    monkeypatch.setattr(httpx, 'AsyncClient', lambda timeout: fake_client)  # noqa: ARG005

    provider = MiniMaxProvider()
    with pytest.raises(RuntimeError) as exc:
        async for _ in provider.stream_synthesize(TTSRequest(text='hello')):
            pass

    assert 'MiniMax API error 1004' in str(exc.value)
