from collections.abc import Iterable

from app.core.settings import settings
from app.tts.base import TTSProvider
from app.tts.models import TTSRequest, TTSResult


class TTSRouter:
    def __init__(self, providers: Iterable[TTSProvider]):
        self.providers = {p.id: p for p in providers}

    async def synthesize_with_provider(self, provider: str, request: TTSRequest) -> TTSResult:
        p = self.providers.get(provider)
        if not p:
            raise ValueError(f'provider {provider!r} is not registered')
        return await p.synthesize(request)

    async def synthesize_with_fallback(self, request: TTSRequest) -> TTSResult:
        errors: list[str] = []
        for provider_id in settings.fallback_order_list():
            provider = self.providers.get(provider_id)
            if not provider:
                continue
            if not provider.is_available():
                errors.append(f'{provider_id}: unavailable')
                continue
            try:
                return await provider.synthesize(request)
            except Exception as exc:  # noqa: BLE001
                detail = str(exc).strip() or exc.__class__.__name__
                errors.append(f'{provider_id}: {detail}')

        raise RuntimeError('all providers failed: ' + '; '.join(errors))
