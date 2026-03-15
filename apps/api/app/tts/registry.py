from app.tts.providers.edge import EdgeTTSProvider
from app.tts.providers.elevenlabs import ElevenLabsProvider
from app.tts.providers.minimax import MiniMaxProvider
from app.tts.router import TTSRouter


def build_tts_router(
    *,
    elevenlabs_api_key: str | None = None,
    elevenlabs_model_id: str | None = None,
    elevenlabs_voice_id: str | None = None,
    minimax_api_key: str | None = None,
    minimax_group_id: str | None = None,
    minimax_model_id: str | None = None,
    minimax_voice_id: str | None = None,
) -> TTSRouter:
    return TTSRouter(
        providers=[
            EdgeTTSProvider(),
            ElevenLabsProvider(
                api_key=elevenlabs_api_key,
                model_id=elevenlabs_model_id,
                voice_id=elevenlabs_voice_id,
            ),
            MiniMaxProvider(
                api_key=minimax_api_key,
                group_id=minimax_group_id,
                model_id=minimax_model_id,
                voice_id=minimax_voice_id,
            ),
        ]
    )
