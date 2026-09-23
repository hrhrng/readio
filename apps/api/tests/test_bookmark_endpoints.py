from fastapi.testclient import TestClient

from app.main import app


client = TestClient(app)


def _mutation(**overrides: object) -> dict[str, object]:
    payload: dict[str, object] = {
        'bookmark_id': 'bookmark-a',
        'char_offset': 4,
        'affinity': 'forward',
        'note': 'First note',
        'created_at_unix_ms': 1_700_000_000_000,
        'updated_at_unix_ms': 1_700_000_000_000,
        'base_version': 0,
        'mutation_id': 'bookmark-mutation-a',
    }
    payload.update(overrides)
    return payload


def test_bookmark_create_is_persisted_and_listed_by_offset() -> None:
    later = client.post(
        '/api/library/items/doc-1/bookmarks',
        json=_mutation(
            bookmark_id='bookmark-b',
            char_offset=10,
            note='Later',
            mutation_id='bookmark-mutation-b',
        ),
    )
    first = client.post('/api/library/items/doc-1/bookmarks', json=_mutation())

    assert later.status_code == 200
    assert first.status_code == 200
    assert first.json()['bookmark'] == {
        'id': 'bookmark-a',
        'item_id': 'doc-1',
        'char_offset': 4,
        'affinity': 'forward',
        'note': 'First note',
        'created_at_unix_ms': 1_700_000_000_000,
        'updated_at_unix_ms': 1_700_000_000_000,
        'version': 1,
    }

    listed = client.get('/api/library/items/doc-1/bookmarks')
    assert listed.status_code == 200
    assert [bookmark['char_offset'] for bookmark in listed.json()] == [4, 10]


def test_same_offset_updates_one_bookmark_and_keeps_its_identity() -> None:
    created = client.post('/api/library/items/doc-1/bookmarks', json=_mutation())
    updated = client.post(
        '/api/library/items/doc-1/bookmarks',
        json=_mutation(
            bookmark_id='bookmark-ignored',
            note='Renamed',
            updated_at_unix_ms=1_700_000_000_100,
            base_version=1,
            mutation_id='bookmark-mutation-rename',
        ),
    )

    assert created.status_code == 200
    assert updated.status_code == 200
    assert updated.json()['bookmark']['id'] == 'bookmark-a'
    assert updated.json()['bookmark']['note'] == 'Renamed'
    assert updated.json()['bookmark']['version'] == 2
    assert len(client.get('/api/library/items/doc-1/bookmarks').json()) == 1


def test_stale_bookmark_update_returns_current_value_as_a_conflict() -> None:
    client.post('/api/library/items/doc-1/bookmarks', json=_mutation())
    stale = client.post(
        '/api/library/items/doc-1/bookmarks',
        json=_mutation(
            note='Stale overwrite',
            base_version=0,
            mutation_id='bookmark-mutation-stale',
        ),
    )

    assert stale.status_code == 409
    assert stale.json()['status'] == 'conflict'
    assert stale.json()['bookmark']['note'] == 'First note'
    assert stale.json()['bookmark']['version'] == 1


def test_bookmark_mutation_replay_is_idempotent() -> None:
    first = client.post('/api/library/items/doc-1/bookmarks', json=_mutation())
    replay = client.post('/api/library/items/doc-1/bookmarks', json=_mutation())

    assert first.status_code == 200
    assert replay.status_code == 200
    assert replay.json()['idempotent_replay'] is True
    assert replay.json()['bookmark']['version'] == 1


def test_bookmark_endpoints_reject_unknown_items_and_invalid_offsets() -> None:
    missing = client.post('/api/library/items/missing/bookmarks', json=_mutation())
    invalid = client.post(
        '/api/library/items/doc-1/bookmarks',
        json=_mutation(char_offset=-1),
    )

    assert missing.status_code == 404
    assert invalid.status_code == 422


def test_bookmark_delete_is_versioned_and_idempotent() -> None:
    client.post('/api/library/items/doc-1/bookmarks', json=_mutation())

    stale = client.delete(
        '/api/library/items/doc-1/bookmarks/bookmark-a',
        params={'base_version': 0, 'mutation_id': 'bookmark-delete-stale'},
    )
    deleted = client.delete(
        '/api/library/items/doc-1/bookmarks/bookmark-a',
        params={'base_version': 1, 'mutation_id': 'bookmark-delete-a'},
    )
    replay = client.delete(
        '/api/library/items/doc-1/bookmarks/bookmark-a',
        params={'base_version': 1, 'mutation_id': 'bookmark-delete-a'},
    )

    assert stale.status_code == 409
    assert deleted.status_code == 204
    assert replay.status_code == 204
    assert client.get('/api/library/items/doc-1/bookmarks').json() == []


def test_bookmark_preserves_the_largest_unicode_scalar_offset() -> None:
    largest_offset = 2**64 - 1

    response = client.post(
        '/api/library/items/doc-1/bookmarks',
        json=_mutation(
            char_offset=largest_offset,
            mutation_id='bookmark-mutation-u64-offset',
        ),
    )

    assert response.status_code == 200
    assert response.json()['bookmark']['char_offset'] == largest_offset
    assert client.get('/api/library/items/doc-1/bookmarks').json()[0]['char_offset'] == largest_offset
