from fastapi.testclient import TestClient

from app.core.settings import settings
from app.main import app


client = TestClient(app)


def test_health_endpoint_returns_ok():
    resp = client.get('/health')
    assert resp.status_code == 200
    assert resp.json() == {'status': 'ok'}


def test_provider_status_lists_required_tts_providers():
    resp = client.get('/api/providers')
    assert resp.status_code == 200
    data = resp.json()

    provider_ids = {p['id'] for p in data['tts']}
    assert provider_ids == {'edge', 'elevenlabs', 'minimax'}


def test_provider_status_marks_minimax_available_with_key_only(monkeypatch):
    monkeypatch.setattr(settings, 'minimax_api_key', 'demo-key')
    monkeypatch.setattr(settings, 'minimax_group_id', '')

    resp = client.get('/api/providers')
    assert resp.status_code == 200
    provider_map = {item['id']: item['available'] for item in resp.json()['tts']}
    assert provider_map['minimax'] is True
