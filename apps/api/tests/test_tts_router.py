import pytest

from app.core.settings import settings
from app.tts.models import TTSRequest, TTSResult
from app.tts.router import TTSRouter


class FakeProvider:
    def __init__(self, provider_id: str, should_fail: bool = False, fail_with_empty_error: bool = False):
        self.provider_id = provider_id
        self.should_fail = should_fail
        self.fail_with_empty_error = fail_with_empty_error

    @property
    def id(self) -> str:
        return self.provider_id

    def is_available(self) -> bool:
        return True

    async def synthesize(self, request: TTSRequest) -> TTSResult:
        if self.should_fail:
            if self.fail_with_empty_error:
                raise RuntimeError()
            raise RuntimeError(f'{self.provider_id} failed')
        return TTSResult(
            provider=self.provider_id,
            audio_bytes=b'test-audio',
            duration_ms=800,
            sample_rate=24000,
            trace_id='trace-test'
        )


@pytest.mark.asyncio
async def test_router_fallback_uses_next_provider_when_first_fails():
    providers = [
        FakeProvider('elevenlabs', should_fail=True),
        FakeProvider('minimax', should_fail=False),
    ]
    router = TTSRouter(providers=providers)

    result = await router.synthesize_with_fallback(
        TTSRequest(text='hello from readio')
    )

    assert result.provider == 'minimax'
    assert result.audio_bytes == b'test-audio'


@pytest.mark.asyncio
async def test_router_can_force_specific_provider():
    providers = [
        FakeProvider('elevenlabs', should_fail=False),
        FakeProvider('minimax', should_fail=False),
    ]
    router = TTSRouter(providers=providers)

    result = await router.synthesize_with_provider(
        provider='elevenlabs',
        request=TTSRequest(text='hello')
    )

    assert result.provider == 'elevenlabs'


@pytest.mark.asyncio
async def test_router_raises_when_provider_missing():
    router = TTSRouter(providers=[FakeProvider('elevenlabs')])

    with pytest.raises(ValueError):
        await router.synthesize_with_provider(
            provider='not-exists',
            request=TTSRequest(text='hello')
        )


@pytest.mark.asyncio
async def test_router_error_message_uses_exception_type_when_error_message_is_empty(monkeypatch):
    router = TTSRouter(
        providers=[
            FakeProvider('edge', should_fail=True, fail_with_empty_error=True),
        ]
    )

    monkeypatch.setattr(settings, 'tts_fallback_order', 'edge')

    with pytest.raises(RuntimeError) as exc:
        await router.synthesize_with_fallback(TTSRequest(text='hello'))

    assert 'edge: RuntimeError' in str(exc.value)
