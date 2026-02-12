from __future__ import annotations

from pathlib import Path

from app.core.settings import settings


def _base_dir() -> Path:
    return Path(settings.library_files_dir)


def save_file(item_id: str, filename: str, data: bytes) -> str:
    """Save file to data/files/{item_id}/{filename}, return relative path."""
    rel = f'{item_id}/{filename}'
    dest = _base_dir() / rel
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(data)
    return rel


def get_file_path(relative_path: str) -> Path:
    """Return absolute path for a relative file path."""
    return _base_dir() / relative_path


def delete_file(relative_path: str) -> bool:
    """Delete a stored file. Returns True if file existed."""
    path = _base_dir() / relative_path
    if path.is_file():
        path.unlink()
        # Remove parent dir if empty
        try:
            path.parent.rmdir()
        except OSError:
            pass
        return True
    return False
