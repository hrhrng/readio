import tempfile
import uuid
from pathlib import Path
from typing import Any

from app.core.settings import settings
from app.tts.models import TTSRequest, TTSResult


def _load_edge_tts_module() -> Any | None:
    try:
        import edge_tts  # type: ignore

        return edge_tts
    except Exception:  # noqa: BLE001
        return None


def _rate_from_speed(speed: float | None) -> str:
    if speed is None:
        return '+0%'

    # Edge TTS rate format uses signed percentages, e.g. "+20%".
    percent = int(round((speed - 1.0) * 100))
    return f'{percent:+d}%'


def _voice_for_language(language: str | None) -> str:
    if not language:
        return settings.edge_tts_voice

    key = language.lower()
    if key.startswith('zh'):
        return 'zh-CN-XiaoxiaoNeural'
    if key.startswith('ja'):
        return 'ja-JP-NanamiNeural'
    if key.startswith('ko'):
        return 'ko-KR-SunHiNeural'
    if key.startswith('es'):
        return 'es-ES-ElviraNeural'
    if key.startswith('fr'):
        return 'fr-FR-DeniseNeural'
    if key.startswith('de'):
        return 'de-DE-KatjaNeural'
    return settings.edge_tts_voice


class EdgeTTSProvider:
    @property
    def id(self) -> str:
        return 'edge'

    def is_available(self) -> bool:
        return _load_edge_tts_module() is not None

    async def synthesize(self, request: TTSRequest) -> TTSResult:
        if request.format != 'mp3':
            raise RuntimeError('edge provider only supports mp3 output')

        edge_tts = _load_edge_tts_module()
        if edge_tts is None:
            raise RuntimeError('edge-tts is not installed. Run: uv sync --project apps/api --all-groups')

        voice = request.voice or _voice_for_language(request.language)
        rate = _rate_from_speed(request.speed)
        communicator = edge_tts.Communicate(text=request.text, voice=voice, rate=rate)

        with tempfile.NamedTemporaryFile(suffix='.mp3', delete=False) as handle:
            temp_path = Path(handle.name)

        try:
            await communicator.save(str(temp_path))
            audio_bytes = temp_path.read_bytes()
        except Exception as exc:  # noqa: BLE001
            raise RuntimeError(f'edge-tts synthesis failed: {exc}') from exc
        finally:
            temp_path.unlink(missing_ok=True)

        if not audio_bytes:
            raise RuntimeError('edge-tts returned empty audio bytes')

        return TTSResult(
            provider=self.id,
            audio_bytes=audio_bytes,
            duration_ms=None,
            sample_rate=None,
            trace_id=str(uuid.uuid4()),
        )
