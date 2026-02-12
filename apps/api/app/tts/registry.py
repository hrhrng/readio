from app.tts.providers.edge import EdgeTTSProvider
from app.tts.providers.elevenlabs import ElevenLabsProvider
from app.tts.providers.minimax import MiniMaxProvider
from app.tts.router import TTSRouter


def build_tts_router() -> TTSRouter:
    return TTSRouter(
        providers=[
            EdgeTTSProvider(),
            ElevenLabsProvider(),
            MiniMaxProvider(),
        ]
    )
