from fastapi.testclient import TestClient

import app.api.routes as routes
from app.main import app


client = TestClient(app)


async def fake_fetch_url_document(url: str):
    return {
        'title': 'Fetched from URL',
        'content': 'This content comes from remote url import.',
    }


async def fake_fetch_url_document_error(url: str):
    raise ValueError('no readable content extracted')


def test_import_url_creates_web_item(monkeypatch):
    monkeypatch.setattr(routes, 'fetch_url_document', fake_fetch_url_document)

    resp = client.post(
        '/api/library/import/url',
        json={
            'url': 'https://example.com/notes',
            'folder_id': 'f1',
            'category': 'imported',
        },
    )

    assert resp.status_code == 201
    data = resp.json()
    assert data['type'] == 'web'
    assert data['title'] == 'Fetched from URL'
    assert data['source'] == 'https://example.com/notes'

    list_resp = client.get('/api/library/items?search=Fetched from URL')
    assert list_resp.status_code == 200
    assert any(item['title'] == 'Fetched from URL' for item in list_resp.json()['items'])


def test_import_url_maps_extract_failure_to_400(monkeypatch):
    monkeypatch.setattr(routes, 'fetch_url_document', fake_fetch_url_document_error)

    resp = client.post(
        '/api/library/import/url',
        json={
            'url': 'https://example.com/empty',
            'folder_id': 'f1',
            'category': 'imported',
        },
    )

    assert resp.status_code == 400
    assert 'no readable content extracted' in resp.json()['detail']
