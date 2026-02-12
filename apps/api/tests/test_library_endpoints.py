import app.api.routes as routes
from fastapi.testclient import TestClient

from app.main import app


client = TestClient(app)


def test_library_items_endpoint_returns_items():
    resp = client.get('/api/library/items')

    assert resp.status_code == 200
    data = resp.json()
    assert 'items' in data
    assert 'total' in data
    assert 'page' in data
    assert 'page_size' in data
    assert 'total_pages' in data
    assert data['page'] == 1
    assert data['page_size'] == 20
    assert len(data['items']) >= 1
    assert data['items'][0]['title']


def test_library_items_support_category_filter():
    resp = client.get('/api/library/items?category=podcasts')

    assert resp.status_code == 200
    data = resp.json()
    assert len(data['items']) >= 1
    assert all(item['category'] == 'podcasts' for item in data['items'])


def test_create_library_item_persists_and_is_searchable():
    create_resp = client.post(
        '/api/library/items',
        json={
            'title': 'Readio API test note',
            'content': 'This item is created from API test.',
            'type': 'txt',
            'category': 'imported',
            'folder_id': 'f1',
            'source': 'API test',
        },
    )
    assert create_resp.status_code == 201

    search_resp = client.get('/api/library/items?search=Readio API test note')
    assert search_resp.status_code == 200
    payload = search_resp.json()
    assert any(item['title'] == 'Readio API test note' for item in payload['items'])


def test_library_items_support_type_sort_and_pagination():
    client.post(
        '/api/library/items',
        json={
            'title': 'Alpha API sort',
            'content': 'alpha',
            'type': 'txt',
            'category': 'imported',
            'folder_id': 'f1',
            'source': 'API test',
        },
    )
    client.post(
        '/api/library/items',
        json={
            'title': 'Zeta API sort',
            'content': 'zeta',
            'type': 'txt',
            'category': 'imported',
            'folder_id': 'f1',
            'source': 'API test',
        },
    )

    type_resp = client.get('/api/library/items?type=txt&page=1&page_size=2&sort_by=title&sort_order=asc')
    assert type_resp.status_code == 200
    type_payload = type_resp.json()
    assert type_payload['page'] == 1
    assert type_payload['page_size'] == 2
    assert type_payload['total'] >= 2
    assert type_payload['total_pages'] >= 1
    assert all(item['type'] == 'txt' for item in type_payload['items'])
    assert type_payload['items'][0]['title'] <= type_payload['items'][-1]['title']

    overflow_resp = client.get('/api/library/items?page=999&page_size=5')
    assert overflow_resp.status_code == 200
    overflow_payload = overflow_resp.json()
    assert overflow_payload['page'] == 999
    assert overflow_payload['total'] >= 1
    assert overflow_payload['total_pages'] >= 1
    assert overflow_payload['items'] == []


def test_delete_library_item_removes_item():
    create_resp = client.post(
        '/api/library/items',
        json={
            'title': 'Readio API delete test',
            'content': 'This item will be deleted from API test.',
            'type': 'txt',
            'category': 'imported',
            'folder_id': 'f1',
            'source': 'API test',
        },
    )
    assert create_resp.status_code == 201
    item_id = create_resp.json()['id']

    delete_resp = client.delete(f'/api/library/items/{item_id}')
    assert delete_resp.status_code == 204

    get_resp = client.get(f'/api/library/items/{item_id}')
    assert get_resp.status_code == 404
