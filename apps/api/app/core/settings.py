import os
from dataclasses import dataclass, field
from pathlib import Path

from dotenv import load_dotenv


REPO_ROOT = Path(__file__).resolve().parents[4]
ROOT_ENV_PATH = REPO_ROOT / '.env'
API_ENV_PATH = REPO_ROOT / 'apps' / 'api' / '.env'

for env_file in (ROOT_ENV_PATH, API_ENV_PATH):
    if env_file.exists():
        load_dotenv(env_file, override=False)


@dataclass
class Settings:
    readio_env: str = field(default_factory=lambda: os.getenv('READIO_ENV', 'dev'))

    elevenlabs_api_key: str = field(default_factory=lambda: os.getenv('ELEVENLABS_API_KEY', ''))
    elevenlabs_base_url: str = field(default_factory=lambda: os.getenv('ELEVENLABS_BASE_URL', 'https://api.elevenlabs.io'))
    elevenlabs_model_id: str = field(default_factory=lambda: os.getenv('ELEVENLABS_MODEL_ID', 'eleven_multilingual_v2'))
    elevenlabs_voice_id: str = field(default_factory=lambda: os.getenv('ELEVENLABS_VOICE_ID', ''))

    minimax_api_key: str = field(default_factory=lambda: os.getenv('MINIMAX_API_KEY', ''))
    minimax_group_id: str = field(default_factory=lambda: os.getenv('MINIMAX_GROUP_ID', ''))
    minimax_api_url: str = field(default_factory=lambda: os.getenv('MINIMAX_API_URL', ''))
    minimax_base_url: str = field(default_factory=lambda: os.getenv('MINIMAX_BASE_URL', 'https://api.minimax.io'))
    minimax_model_id: str = field(default_factory=lambda: os.getenv('MINIMAX_MODEL_ID', 'speech-2.6-hd'))
    minimax_voice_id: str = field(default_factory=lambda: os.getenv('MINIMAX_VOICE_ID', 'English_expressive_narrator'))
    minimax_default_language_boost: str = field(
        default_factory=lambda: os.getenv('MINIMAX_DEFAULT_LANGUAGE_BOOST', '')
    )
    edge_tts_voice: str = field(default_factory=lambda: os.getenv('EDGE_TTS_VOICE', 'en-US-AriaNeural'))

    tts_fallback_order: str = field(default_factory=lambda: os.getenv('TTS_FALLBACK_ORDER', 'edge,minimax,elevenlabs'))
    tts_timeout_seconds: float = field(default_factory=lambda: float(os.getenv('TTS_TIMEOUT_SECONDS', '20')))
    tts_job_parallel_limit: int = field(default_factory=lambda: int(os.getenv('TTS_JOB_PARALLEL_LIMIT', '2')))
    tts_job_cache_size: int = field(default_factory=lambda: int(os.getenv('TTS_JOB_CACHE_SIZE', '128')))
    library_import_timeout_seconds: float = field(
        default_factory=lambda: float(os.getenv('READIO_LIBRARY_IMPORT_TIMEOUT_SECONDS', '60'))
    )
    library_import_max_bytes: int = field(
        default_factory=lambda: int(os.getenv('READIO_LIBRARY_IMPORT_MAX_BYTES', str(30 * 1024 * 1024)))
    )
    library_embedded_image_max_bytes: int = field(
        default_factory=lambda: int(os.getenv('READIO_LIBRARY_EMBEDDED_IMAGE_MAX_BYTES', str(220 * 1024)))
    )
    library_epub_chapter_image_limit: int = field(
        default_factory=lambda: int(os.getenv('READIO_LIBRARY_EPUB_CHAPTER_IMAGE_LIMIT', '8'))
    )
    library_epub_max_chars: int = field(
        default_factory=lambda: int(os.getenv('READIO_LIBRARY_EPUB_MAX_CHARS', str(6_000_000)))
    )
    library_db_path: str = field(default_factory=lambda: os.getenv('READIO_LIBRARY_DB_PATH', './data/readio.db'))
    library_files_dir: str = field(default_factory=lambda: os.getenv('READIO_LIBRARY_FILES_DIR', './data/files'))
    cors_origins: str = field(
        default_factory=lambda: os.getenv(
            'READIO_CORS_ORIGINS',
            'http://localhost:3000,http://127.0.0.1:3000',
        )
    )

    def fallback_order_list(self) -> list[str]:
        return [x.strip() for x in self.tts_fallback_order.split(',') if x.strip()]

    def cors_origins_list(self) -> list[str]:
        return [x.strip() for x in self.cors_origins.split(',') if x.strip()]


settings = Settings()
