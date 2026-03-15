import hashlib
import logging
import time
import uuid
from typing import Any

import httpx

from app.core.settings import settings
from app.tts.models import TTSRequest, TTSResult

logger = logging.getLogger(__name__)

# In-memory cache for ElevenLabs voices, keyed by hashed API key.
# Dict of {key_hash: (timestamp, voice_list)} — refreshed every hour.
_elevenlabs_voice_cache: dict[str, tuple[float, list[dict[str, Any]]]] = {}
_ELEVENLABS_VOICE_CACHE_TTL = 3600  # 1 hour


async def fetch_elevenlabs_voices(api_key: str) -> list[dict[str, Any]]:
    """Fetch voices from ElevenLabs GET /v1/voices.

    Results are cached in memory for 1 hour per API key hash.
    Returns a list of dicts with keys: voice_id, name, labels.
    """
    key_hash = hashlib.sha256(api_key.encode()).hexdigest()[:16]

    cached = _elevenlabs_voice_cache.get(key_hash)
    if cached is not None:
        cached_at, cached_voices = cached
        if time.monotonic() - cached_at < _ELEVENLABS_VOICE_CACHE_TTL:
            return cached_voices

    base_url = settings.elevenlabs_base_url.rstrip('/')
    url = f'{base_url}/v1/voices'
    headers = {'xi-api-key': api_key}

    try:
        async with httpx.AsyncClient(timeout=30) as client:
            resp = await client.get(url, headers=headers)
            resp.raise_for_status()
            data = resp.json()
    except Exception:
        logger.exception('Failed to fetch ElevenLabs voices')
        if cached is not None:
            return cached[1]
        return []

    voices_raw = data.get('voices', [])
    voices = [
        {
            'voice_id': v.get('voice_id', ''),
            'name': v.get('name', ''),
            'labels': v.get('labels') or {},
        }
        for v in voices_raw
        if isinstance(v, dict)
    ]

    _elevenlabs_voice_cache[key_hash] = (time.monotonic(), voices)
    logger.info('Fetched %d voices from ElevenLabs API', len(voices))
    return voices


class ElevenLabsProvider:
    def __init__(
        self,
        *,
        api_key: str | None = None,
        model_id: str | None = None,
        voice_id: str | None = None,
    ):
        self._user_api_key = api_key
        self._user_model_id = model_id
        self._user_voice_id = voice_id

    @property
    def id(self) -> str:
        return 'elevenlabs'

    def _effective_api_key(self) -> str:
        return self._user_api_key or settings.elevenlabs_api_key

    def _effective_model_id(self) -> str:
        return self._user_model_id or settings.elevenlabs_model_id

    def _effective_voice_id(self, request: TTSRequest) -> str:
        return request.voice or self._user_voice_id or settings.elevenlabs_voice_id

    def is_available(self) -> bool:
        return bool(self._effective_api_key())

    async def synthesize(self, request: TTSRequest) -> TTSResult:
        api_key = self._effective_api_key()
        if not api_key:
            raise RuntimeError('ELEVENLABS_API_KEY is not configured')

        voice_id = self._effective_voice_id(request)
        if not voice_id:
            raise RuntimeError('voice is required for ElevenLabs provider')

        url = f"{settings.elevenlabs_base_url}/v1/text-to-speech/{voice_id}"
        headers = {
            'xi-api-key': api_key,
            'accept': 'audio/mpeg' if request.format == 'mp3' else 'audio/wav',
            'content-type': 'application/json'
        }

        payload = {
            'text': request.text,
            'model_id': self._effective_model_id(),
            'voice_settings': {
                'stability': request.provider_options.get('stability', 0.5),
                'similarity_boost': request.provider_options.get('similarity_boost', 0.75)
            }
        }

        async with httpx.AsyncClient(timeout=settings.tts_timeout_seconds) as client:
            response = await client.post(url, headers=headers, json=payload)
            response.raise_for_status()

        return TTSResult(
            provider=self.id,
            audio_bytes=response.content,
            duration_ms=None,
            sample_rate=None,
            trace_id=str(uuid.uuid4())
        )
