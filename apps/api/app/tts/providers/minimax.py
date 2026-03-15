import base64
import json
import logging
import re
import time
import uuid
from dataclasses import dataclass
from typing import Any
from collections.abc import AsyncIterator
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

import httpx

from app.core.settings import settings
from app.tts.models import TTSRequest, TTSResult

logger = logging.getLogger(__name__)

DEFAULT_MINIMAX_MODEL = 'speech-2.6-hd'
DEFAULT_MINIMAX_VOICE = 'English_expressive_narrator'

# In-memory cache for system voices fetched from MiniMax API.
# Tuple of (timestamp, voice_list) — refreshed every hour.
_voice_cache: tuple[float, list[dict[str, Any]]] | None = None
_VOICE_CACHE_TTL = 3600  # 1 hour


async def fetch_system_voices() -> list[dict[str, Any]]:
    """Call MiniMax POST /v1/get_voice to retrieve all system voices.

    Results are cached in memory for 1 hour to avoid redundant API calls.
    Returns a list of dicts with keys: voice_id, voice_name, description.
    """
    global _voice_cache

    # Return cached result if still fresh
    if _voice_cache is not None:
        cached_at, cached_voices = _voice_cache
        if time.monotonic() - cached_at < _VOICE_CACHE_TTL:
            return cached_voices

    api_key = settings.minimax_api_key
    if not api_key:
        logger.warning('MINIMAX_API_KEY not set — returning empty voice list')
        return []

    base_url = settings.minimax_base_url.rstrip('/')
    url = f'{base_url}/v1/get_voice'

    # Append GroupId if configured (same pattern as TTS endpoint)
    group_id = settings.minimax_group_id.strip()
    if group_id:
        url = _append_group_id(url, group_id)

    headers = {
        'Authorization': f'Bearer {api_key}',
        'Content-Type': 'application/json',
    }
    body = {'voice_type': 'system'}

    try:
        async with httpx.AsyncClient(timeout=30) as client:
            resp = await client.post(url, headers=headers, json=body)
            resp.raise_for_status()
            data = resp.json()
    except Exception:
        logger.exception('Failed to fetch MiniMax system voices')
        # Return stale cache if available, otherwise empty
        if _voice_cache is not None:
            return _voice_cache[1]
        return []

    # MiniMax response shape: {"system_voice": [{"voice_id": "...", ...}, ...]}
    voices = data.get('system_voice', [])
    if not isinstance(voices, list):
        logger.error('Unexpected MiniMax get_voice response: %s', type(voices))
        return []

    _voice_cache = (time.monotonic(), voices)
    logger.info('Fetched %d system voices from MiniMax API', len(voices))
    return voices


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


def _resolve_voice_id(request: TTSRequest, user_voice_id: str | None = None) -> str:
    return request.voice or user_voice_id or settings.minimax_voice_id or DEFAULT_MINIMAX_VOICE


def _build_payload(
    request: TTSRequest,
    *,
    stream: bool,
    user_model_id: str | None = None,
    user_voice_id: str | None = None,
    user_group_id: str | None = None,
) -> dict[str, Any]:
    payload: dict[str, Any] = {
        'model': request.provider_options.get('model') or user_model_id or settings.minimax_model_id or DEFAULT_MINIMAX_MODEL,
        'text': request.text,
        'stream': stream,
        'voice_setting': {
            'voice_id': _resolve_voice_id(request, user_voice_id),
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
    group_id = (user_group_id or settings.minimax_group_id).strip()
    if group_id:
        payload['group_id'] = group_id
    return payload


def _build_enhanced_payload(request: TTSRequest, *, stream: bool) -> dict[str, Any]:
    """Build a MiniMax payload with all available prosody/control parameters.

    Starts from the base payload and layers on optional parameters from
    ``request.provider_options``:

    - **emotion** — happy / sad / angry / fearful / disgusted / surprised / neutral
    - **language_boost** — ISO language name for multilingual emphasis
    - **pronunciation_dict** — custom pronunciation map, e.g. ``{"tone": ["omg/oh my god"]}``
    - **text_normalization** — normalise numbers/dates in English text (default True)
    - **voice_modify** — fine-grained pitch / intensity / timbre / sound_effects
    - **subtitle_enable** — return sentence-level timestamps (non-streaming only)
    """
    payload = _build_payload(request, stream=stream)

    opts = request.provider_options

    # emotion: vocal emotion preset
    emotion = opts.get('emotion')
    if emotion:
        payload['voice_setting']['emotion'] = emotion

    # language_boost: multilingual enhancement (fall back to env default)
    language_boost = opts.get('language_boost') or settings.minimax_default_language_boost
    if language_boost:
        payload['language_boost'] = language_boost

    # pronunciation_dict: custom pronunciation overrides
    pronunciation_dict = opts.get('pronunciation_dict')
    if pronunciation_dict:
        payload['pronunciation_dict'] = pronunciation_dict

    # text_normalization: English number/date normalisation (enabled by default)
    payload['voice_setting']['text_normalization'] = opts.get('text_normalization', True)

    # voice_modify: granular voice tuning (pitch / intensity / timbre / sound_effects)
    voice_modify = opts.get('voice_modify')
    if voice_modify:
        payload['voice_modify'] = voice_modify

    # subtitle_enable: sentence-level timestamps — only meaningful for non-streaming
    if not stream:
        payload['subtitle_enable'] = opts.get('subtitle_enable', False)

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
    def __init__(
        self,
        *,
        api_key: str | None = None,
        group_id: str | None = None,
        model_id: str | None = None,
        voice_id: str | None = None,
    ):
        self._user_api_key = api_key
        self._user_group_id = group_id
        self._user_model_id = model_id
        self._user_voice_id = voice_id

    @property
    def id(self) -> str:
        return 'minimax'

    def _effective_api_key(self) -> str:
        return self._user_api_key or settings.minimax_api_key

    def is_available(self) -> bool:
        return bool(self._effective_api_key())

    async def synthesize(self, request: TTSRequest) -> TTSResult:
        if not self.is_available():
            raise RuntimeError('MINIMAX_API_KEY is not configured')

        url = resolve_minimax_endpoint()
        api_key = self._effective_api_key()
        headers = {
            'Authorization': f"Bearer {api_key}",
            'Content-Type': 'application/json'
        }
        payload = _build_payload(
            request, stream=False,
            user_model_id=self._user_model_id,
            user_voice_id=self._user_voice_id,
            user_group_id=self._user_group_id,
        )

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

        # MiniMax returns duration and sample rate in `extra_info`, not in `data`.
        extra = data.get('extra_info') or {}
        return TTSResult(
            provider=self.id,
            audio_bytes=audio_bytes,
            duration_ms=extra.get('audio_length'),
            sample_rate=extra.get('audio_sample_rate'),
            trace_id=_extract_trace_id(data),
        )

    async def synthesize_enhanced(self, request: TTSRequest) -> TTSResult:
        """Enhanced synthesis with full prosody control.

        Uses ``_build_enhanced_payload`` to enable emotion, language_boost,
        pronunciation_dict, text_normalization, voice_modify, and subtitle
        parameters — all driven by ``request.provider_options``.
        """
        if not self.is_available():
            raise RuntimeError('MINIMAX_API_KEY is not configured')

        url = resolve_minimax_endpoint()
        api_key = self._effective_api_key()
        headers = {
            'Authorization': f'Bearer {api_key}',
            'Content-Type': 'application/json',
        }
        payload = _build_enhanced_payload(request, stream=False)

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

        extra = data.get('extra_info') or {}
        return TTSResult(
            provider=self.id,
            audio_bytes=audio_bytes,
            duration_ms=extra.get('audio_length'),
            sample_rate=extra.get('audio_sample_rate'),
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
        api_key = self._effective_api_key()
        headers = {
            'Authorization': f"Bearer {api_key}",
            'Content-Type': 'application/json',
        }
        payload = _build_payload(
            request, stream=True,
            user_model_id=self._user_model_id,
            user_voice_id=self._user_voice_id,
            user_group_id=self._user_group_id,
        )
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
