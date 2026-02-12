import uuid

import httpx

from app.core.settings import settings
from app.tts.models import TTSRequest, TTSResult


class ElevenLabsProvider:
    @property
    def id(self) -> str:
        return 'elevenlabs'

    def is_available(self) -> bool:
        return bool(settings.elevenlabs_api_key)

    async def synthesize(self, request: TTSRequest) -> TTSResult:
        if not self.is_available():
            raise RuntimeError('ELEVENLABS_API_KEY is not configured')

        voice_id = request.voice or settings.elevenlabs_voice_id
        if not voice_id:
            raise RuntimeError('voice is required for ElevenLabs provider')

        url = f"{settings.elevenlabs_base_url}/v1/text-to-speech/{voice_id}"
        headers = {
            'xi-api-key': settings.elevenlabs_api_key,
            'accept': 'audio/mpeg' if request.format == 'mp3' else 'audio/wav',
            'content-type': 'application/json'
        }

        payload = {
            'text': request.text,
            'model_id': settings.elevenlabs_model_id,
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
