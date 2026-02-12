import base64
import json
import re
import uuid
from dataclasses import dataclass
from typing import Any
from collections.abc import AsyncIterator
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

import httpx

from app.core.settings import settings
from app.tts.models import TTSRequest, TTSResult

DEFAULT_MINIMAX_MODEL = 'speech-2.6-hd'
DEFAULT_MINIMAX_VOICE = 'English_expressive_narrator'


@dataclass
class MiniMaxStreamChunk:
    audio_bytes: bytes
    trace_id: str
    status: int | str | None = None


def decode_audio_payload(payload: str) -> bytes:
    value = payload.strip()
    if not value:
        raise RuntimeError('MiniMax audio payload is empty')

    # Some accounts return raw hex bytes, others return base64.
    hex_candidate = value[2:] if value.startswith('0x') else value
    if len(hex_candidate) % 2 == 0 and re.fullmatch(r'[0-9a-fA-F]+', hex_candidate):
        return bytes.fromhex(hex_candidate)

    try:
        return base64.b64decode(value, validate=True)
    except Exception as exc:  # noqa: BLE001
        raise RuntimeError('MiniMax audio payload is neither hex nor base64') from exc


def _append_group_id(url: str, group_id: str) -> str:
    if not group_id:
        return url

    split = urlsplit(url)
    query = dict(parse_qsl(split.query, keep_blank_values=True))
    if 'GroupId' in query or 'group_id' in query:
        return url

    query['GroupId'] = group_id
    return urlunsplit((split.scheme, split.netloc, split.path, urlencode(query), split.fragment))


def resolve_minimax_endpoint() -> str:
    explicit_api_url = settings.minimax_api_url.strip()
    if explicit_api_url:
        return _append_group_id(explicit_api_url, settings.minimax_group_id.strip())

    base = settings.minimax_base_url.rstrip('/')
    if base.endswith('/v1/t2a_v2'):
        return _append_group_id(base, settings.minimax_group_id.strip())

    return _append_group_id(f'{base}/v1/t2a_v2', settings.minimax_group_id.strip())


def _resolve_voice_id(request: TTSRequest) -> str:
    return request.voice or settings.minimax_voice_id or DEFAULT_MINIMAX_VOICE


def _build_payload(request: TTSRequest, *, stream: bool) -> dict[str, Any]:
    payload: dict[str, Any] = {
        'model': request.provider_options.get('model') or settings.minimax_model_id or DEFAULT_MINIMAX_MODEL,
        'text': request.text,
        'stream': stream,
        'voice_setting': {
            'voice_id': _resolve_voice_id(request),
            'speed': request.speed if request.speed is not None else 1.0,
            'vol': 1.0,
            'pitch': 0,
        },
        'audio_setting': {
            'sample_rate': 32000,
            'bitrate': 128000,
            'format': request.format,
            'channel': 1,
        },
        'output_format': request.provider_options.get('output_format', 'hex'),
    }
    group_id = settings.minimax_group_id.strip()
    if group_id:
        payload['group_id'] = group_id
    return payload


def _raise_if_minimax_error(data: dict[str, Any]) -> None:
    base_resp = data.get('base_resp', {})
    status_code = base_resp.get('status_code')
    if status_code not in (None, 0, '0'):
        status_msg = base_resp.get('status_msg') or 'unknown error'
        raise RuntimeError(f'MiniMax API error {status_code}: {status_msg}')


def _extract_audio_payload(data: dict[str, Any]) -> str | None:
    nested = data.get('data')
    if isinstance(nested, dict):
        audio = nested.get('audio')
        if isinstance(audio, str):
            return audio

    top_audio = data.get('audio')
    if isinstance(top_audio, str):
        return top_audio
    return None


def _extract_trace_id(data: dict[str, Any]) -> str:
    return str(data.get('trace_id') or data.get('request_id') or uuid.uuid4())


class MiniMaxProvider:
    @property
    def id(self) -> str:
        return 'minimax'

    def is_available(self) -> bool:
        return bool(settings.minimax_api_key)

    async def synthesize(self, request: TTSRequest) -> TTSResult:
        if not self.is_available():
            raise RuntimeError('MINIMAX_API_KEY is not configured')

        url = resolve_minimax_endpoint()
        headers = {
            'Authorization': f"Bearer {settings.minimax_api_key}",
            'Content-Type': 'application/json'
        }
        payload = _build_payload(request, stream=False)

        try:
            async with httpx.AsyncClient(timeout=settings.tts_timeout_seconds) as client:
                response = await client.post(url, headers=headers, json=payload)
                response.raise_for_status()
        except httpx.HTTPStatusError as exc:
            detail = exc.response.text.strip() or exc.response.reason_phrase
            raise RuntimeError(f'MiniMax service HTTP {exc.response.status_code}: {detail}') from exc
        except httpx.RequestError as exc:
            raise RuntimeError(f'MiniMax request failed: {exc}') from exc

        try:
            data = response.json()
        except Exception as exc:  # noqa: BLE001
            raise RuntimeError('MiniMax response is not valid JSON') from exc

        _raise_if_minimax_error(data)
        audio_payload = _extract_audio_payload(data)
        if not audio_payload:
            raise RuntimeError('MiniMax response has no audio payload')

        audio_bytes = decode_audio_payload(audio_payload)

        return TTSResult(
            provider=self.id,
            audio_bytes=audio_bytes,
            duration_ms=(data.get('data') or {}).get('audio_length_ms') if isinstance(data.get('data'), dict) else None,
            sample_rate=(data.get('data') or {}).get('sample_rate') if isinstance(data.get('data'), dict) else None,
            trace_id=_extract_trace_id(data),
        )

    async def _iter_stream_payloads(self, response: Any) -> AsyncIterator[dict[str, Any]]:
        buffered_data_lines: list[str] = []

        async for raw_line in response.aiter_lines():
            line = raw_line.strip()
            if not line:
                if buffered_data_lines:
                    payload_text = '\n'.join(buffered_data_lines).strip()
                    buffered_data_lines.clear()
                    if payload_text and payload_text != '[DONE]':
                        try:
                            yield json.loads(payload_text)
                        except json.JSONDecodeError:
                            continue
                continue

            if line.startswith(':'):
                continue
            if line.startswith('event:'):
                continue
            if line.startswith('data:'):
                buffered_data_lines.append(line[len('data:') :].strip())
                continue
            if line == '[DONE]':
                continue

            try:
                yield json.loads(line)
            except json.JSONDecodeError:
                continue

        if buffered_data_lines:
            payload_text = '\n'.join(buffered_data_lines).strip()
            if payload_text and payload_text != '[DONE]':
                try:
                    yield json.loads(payload_text)
                except json.JSONDecodeError:
                    return

    async def stream_synthesize(self, request: TTSRequest) -> AsyncIterator[MiniMaxStreamChunk]:
        if not self.is_available():
            raise RuntimeError('MINIMAX_API_KEY is not configured')

        url = resolve_minimax_endpoint()
        headers = {
            'Authorization': f"Bearer {settings.minimax_api_key}",
            'Content-Type': 'application/json',
        }
        payload = _build_payload(request, stream=True)
        # MiniMax streaming is audio-chunk streaming. Hex is stable across accounts.
        payload['output_format'] = 'hex'

        timeout = httpx.Timeout(
            connect=settings.tts_timeout_seconds,
            read=None,
            write=settings.tts_timeout_seconds,
            pool=settings.tts_timeout_seconds,
        )

        saw_audio = False
        try:
            async with httpx.AsyncClient(timeout=timeout) as client:
                async with client.stream('POST', url, headers=headers, json=payload) as response:
                    response.raise_for_status()

                    async for data in self._iter_stream_payloads(response):
                        _raise_if_minimax_error(data)
                        audio_payload = _extract_audio_payload(data)
                        if not audio_payload:
                            continue

                        audio_bytes = decode_audio_payload(audio_payload)
                        if not audio_bytes:
                            continue

                        saw_audio = True
                        chunk_status = None
                        nested = data.get('data')
                        if isinstance(nested, dict):
                            chunk_status = nested.get('status')

                        yield MiniMaxStreamChunk(
                            audio_bytes=audio_bytes,
                            trace_id=_extract_trace_id(data),
                            status=chunk_status,
                        )
        except httpx.HTTPStatusError as exc:
            detail = exc.response.text.strip() or exc.response.reason_phrase
            raise RuntimeError(f'MiniMax service HTTP {exc.response.status_code}: {detail}') from exc
        except httpx.RequestError as exc:
            raise RuntimeError(f'MiniMax request failed: {exc}') from exc

        if not saw_audio:
            raise RuntimeError('MiniMax streaming response has no audio payload')
