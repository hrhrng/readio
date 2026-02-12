import pytest

import app.api.routes as routes
from app.library.store import LibraryStore


@pytest.fixture(autouse=True)
def isolated_library_store(tmp_path, monkeypatch):
    store = LibraryStore(str(tmp_path / 'readio-test.db'))
    monkeypatch.setattr(routes, 'library_store', store)
