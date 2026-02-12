from app.tts.registry import build_tts_router


def test_registry_registers_expected_providers():
    router = build_tts_router()
    assert set(router.providers.keys()) == {'edge', 'elevenlabs', 'minimax'}
