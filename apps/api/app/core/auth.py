"""Authentication helpers for FastAPI routes.

Validates Better Auth session cookies by querying the auth.db directly.
"""

from __future__ import annotations

import sqlite3
from datetime import UTC, datetime
from pathlib import Path

from fastapi import HTTPException, Request

from app.core.settings import settings


def _get_auth_db() -> sqlite3.Connection:
    db_path = Path(settings.auth_db_path)
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    return conn


def get_current_user_id(request: Request) -> str:
    """FastAPI dependency: extract and validate session, return userId.

    Raises 401 if no valid session.
    """
    token = request.cookies.get("better-auth.session_token")
    if not token:
        raise HTTPException(status_code=401, detail="not authenticated")

    try:
        with _get_auth_db() as conn:
            row = conn.execute(
                "SELECT userId, expiresAt FROM session WHERE token = ?",
                (token,),
            ).fetchone()
    except Exception:
        raise HTTPException(status_code=401, detail="not authenticated")

    if row is None:
        raise HTTPException(status_code=401, detail="invalid session")

    # Check expiry
    expires_at = row["expiresAt"]
    if isinstance(expires_at, str):
        try:
            exp = datetime.fromisoformat(expires_at.replace("Z", "+00:00"))
            if exp < datetime.now(UTC):
                raise HTTPException(status_code=401, detail="session expired")
        except ValueError:
            pass

    return row["userId"]


def get_optional_user_id(request: Request) -> str | None:
    """Like get_current_user_id but returns None instead of raising 401."""
    try:
        return get_current_user_id(request)
    except HTTPException:
        return None
