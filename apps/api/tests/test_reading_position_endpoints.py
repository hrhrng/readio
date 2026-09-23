from fastapi.testclient import TestClient

from app.main import app


client = TestClient(app)


def _mutation(**overrides: object) -> dict[str, object]:
    payload: dict[str, object] = {
        'document_revision': 'revision-1',
        'char_offset': 4,
        'affinity': 'forward',
        'observed_at_unix_ms': 1_700_000_000_000,
        'session_id': 'session-a',
        'mutation_id': 'mutation-a',
        'base_version': 0,
    }
    payload.update(overrides)
    return payload


def test_position_is_absent_until_the_first_authoritative_mutation() -> None:
    response = client.get('/api/library/items/doc-1/position')

    assert response.status_code == 404


def test_position_mutation_is_versioned_and_readable() -> None:
    response = client.post(
        '/api/library/items/doc-1/position',
        json=_mutation(),
    )

    assert response.status_code == 200
    assert response.json() == {
        'status': 'applied',
        'idempotent_replay': False,
        'version': 1,
        'position': {
            **_mutation(),
            'version': 1,
        },
        'conflict_id': None,
        'candidates': [],
    }

    stored = client.get('/api/library/items/doc-1/position')
    assert stored.status_code == 200
    assert stored.json() == {**_mutation(), 'version': 1}


def test_replaying_a_mutation_id_is_idempotent() -> None:
    first = client.post('/api/library/items/doc-1/position', json=_mutation())
    replay = client.post('/api/library/items/doc-1/position', json=_mutation())

    assert first.status_code == 200
    assert replay.status_code == 200
    assert replay.json()['status'] == 'applied'
    assert replay.json()['idempotent_replay'] is True
    assert replay.json()['version'] == 1
    assert client.get('/api/library/items/doc-1/position').json()['version'] == 1


def test_reusing_a_mutation_id_with_a_different_payload_is_rejected() -> None:
    first = client.post('/api/library/items/doc-1/position', json=_mutation())
    changed = client.post(
        '/api/library/items/doc-1/position',
        json=_mutation(char_offset=5),
    )

    assert first.status_code == 200
    assert changed.status_code == 409
    assert 'different position payload' in changed.json()['detail']


def test_divergent_base_version_preserves_both_position_candidates() -> None:
    first = client.post('/api/library/items/doc-1/position', json=_mutation())
    assert first.status_code == 200

    incoming = _mutation(
        char_offset=1,
        affinity='backward',
        session_id='session-b',
        mutation_id='mutation-b',
    )
    conflict = client.post('/api/library/items/doc-1/position', json=incoming)

    assert conflict.status_code == 409
    payload = conflict.json()
    assert payload['status'] == 'conflict'
    assert payload['version'] == 1
    assert payload['conflict_id']
    assert [candidate['origin'] for candidate in payload['candidates']] == [
        'current',
        'incoming',
    ]
    assert [candidate['char_offset'] for candidate in payload['candidates']] == [4, 1]
    assert client.get('/api/library/items/doc-1/position').json()['char_offset'] == 4


def test_conflict_replay_returns_the_same_conflict_without_duplicating_it() -> None:
    client.post('/api/library/items/doc-1/position', json=_mutation())
    incoming = _mutation(
        char_offset=1,
        session_id='session-b',
        mutation_id='mutation-b',
    )

    first = client.post('/api/library/items/doc-1/position', json=incoming)
    replay = client.post('/api/library/items/doc-1/position', json=incoming)

    assert first.status_code == 409
    assert replay.status_code == 409
    assert replay.json()['idempotent_replay'] is True
    assert replay.json()['conflict_id'] == first.json()['conflict_id']


def test_position_mutation_rejects_unknown_items_and_invalid_offsets() -> None:
    missing = client.post('/api/library/items/missing/position', json=_mutation())
    invalid = client.post(
        '/api/library/items/doc-1/position',
        json=_mutation(char_offset=-1),
    )

    assert missing.status_code == 404
    assert invalid.status_code == 422


def test_position_preserves_the_largest_unicode_scalar_offset() -> None:
    largest_offset = 2**64 - 1

    response = client.post(
        '/api/library/items/doc-1/position',
        json=_mutation(
            char_offset=largest_offset,
            mutation_id='mutation-u64-offset',
        ),
    )

    assert response.status_code == 200
    assert response.json()['position']['char_offset'] == largest_offset
    assert client.get('/api/library/items/doc-1/position').json()['char_offset'] == largest_offset


def test_position_conflict_preserves_the_largest_base_version() -> None:
    largest_version = 2**64 - 1

    response = client.post(
        '/api/library/items/doc-1/position',
        json=_mutation(
            base_version=largest_version,
            mutation_id='mutation-u64-version',
        ),
    )

    assert response.status_code == 409
    incoming = response.json()['candidates'][0]
    assert incoming['origin'] == 'incoming'
    assert incoming['base_version'] == largest_version
