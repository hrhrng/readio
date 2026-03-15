from __future__ import annotations

import math
import sqlite3
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4

from app.library.models import CreateLibraryItemRequest, LibraryItem, LibraryItemSummary


def _format_library_date(now: datetime) -> str:
    return f"{now.strftime('%b')} {now.day}, {now.year}"


class LibraryStore:
    def __init__(self, db_path: str):
        self.db_path = Path(db_path)
        self._initialize()

    def _connect(self) -> sqlite3.Connection:
        connection = sqlite3.connect(self.db_path)
        connection.row_factory = sqlite3.Row
        return connection

    def _initialize(self) -> None:
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        with self._connect() as connection:
            connection.execute(
                '''
                CREATE TABLE IF NOT EXISTS library_items (
                    id TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    content TEXT NOT NULL,
                    type TEXT NOT NULL,
                    progress INTEGER NOT NULL,
                    date TEXT NOT NULL,
                    category TEXT NOT NULL,
                    folder_id TEXT NOT NULL,
                    source TEXT NOT NULL,
                    created_at TEXT NOT NULL
                )
                '''
            )
            connection.commit()

            # Migrate: add file_path column if missing
            columns = [
                row['name']
                for row in connection.execute('PRAGMA table_info(library_items)').fetchall()
            ]
            if 'file_path' not in columns:
                connection.execute('ALTER TABLE library_items ADD COLUMN file_path TEXT DEFAULT NULL')
                connection.commit()

            if 'cover_image' not in columns:
                connection.execute('ALTER TABLE library_items ADD COLUMN cover_image TEXT DEFAULT NULL')
                connection.commit()

                # Backfill: extract cover_image from content JSON for existing EPUB items
                import json as _json
                rows = connection.execute(
                    "SELECT id, content FROM library_items WHERE type = 'epub'"
                ).fetchall()
                for row in rows:
                    try:
                        payload = _json.loads(row['content'])
                        cover = payload.get('cover_image')
                        if cover:
                            connection.execute(
                                'UPDATE library_items SET cover_image = ? WHERE id = ?',
                                (cover, row['id']),
                            )
                    except Exception:  # noqa: BLE001
                        pass
                connection.commit()

            # Migrate: add voice column for per-item TTS voice preference
            if 'voice' not in columns:
                connection.execute('ALTER TABLE library_items ADD COLUMN voice TEXT DEFAULT NULL')
                connection.commit()

            # Migrate: add per-book playback speed preference
            if 'speed' not in columns:
                connection.execute('ALTER TABLE library_items ADD COLUMN speed REAL DEFAULT NULL')
                connection.commit()

            # Migrate: add current chapter position tracking for EPUB reading
            if 'current_chapter' not in columns:
                connection.execute('ALTER TABLE library_items ADD COLUMN current_chapter TEXT DEFAULT NULL')
                connection.commit()

            # Migrate: add content_format to distinguish markdown vs plaintext web imports
            if 'content_format' not in columns:
                connection.execute('ALTER TABLE library_items ADD COLUMN content_format TEXT DEFAULT NULL')
                connection.commit()

            # Settings table: single-row key-value store for user preferences
            # (font_size, accent_color, tts_speed, etc.)
            connection.execute(
                '''
                CREATE TABLE IF NOT EXISTS settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                )
                '''
            )
            connection.commit()

            # Per-user settings table for per-user API keys and preferences
            connection.execute(
                '''
                CREATE TABLE IF NOT EXISTS user_settings (
                    user_id TEXT NOT NULL,
                    key TEXT NOT NULL,
                    value TEXT NOT NULL,
                    PRIMARY KEY (user_id, key)
                )
                '''
            )
            connection.commit()

        self._seed_defaults_if_needed()

    def _seed_defaults_if_needed(self) -> None:
        with self._connect() as connection:
            current_count = int(connection.execute('SELECT COUNT(*) AS count FROM library_items').fetchone()['count'])
            if current_count > 0:
                return

            defaults = [
                {
                    'id': 'doc-1',
                    'title': 'naturalreader - Google 搜索',
                    'content': 'Speechify-like reader benchmark notes.\n\nNaturalReader focuses on website reading, but the conversion workflow for highlighting + chat should be smoother in Readio.\n\nOpen-source opportunity: local-first parsing, reproducible voice routing, and transparent provider fallback logs.',
                    'type': 'web',
                    'progress': 0,
                    'date': 'May 28, 2025',
                    'category': 'imported',
                    'folder_id': 'f1',
                    'source': 'Web import',
                    'created_at': '2025-05-28T09:00:00Z',
                },
                {
                    'id': 'doc-2',
                    'title': 'Streamlabs Overview',
                    'content': 'Streamlabs product overview in Chinese.\n\nAI coding 对 Agent 来说是后互联网时代最泛化的 tool。\n\n如果 Readio 需要支持 Agent 工作流，阅读体验必须覆盖：高亮解释、可追溯的总结、可分享的音频片段。',
                    'type': 'web',
                    'progress': 94,
                    'date': 'Apr 10, 2025',
                    'category': 'imported',
                    'folder_id': 'f1',
                    'source': 'Web import',
                    'created_at': '2025-04-10T09:00:00Z',
                },
                {
                    'id': 'doc-3',
                    'title': '测试Speechify',
                    'content': 'Headless browser and human browser parity matters.\n\naccess all things build for humanity.\n\nReadio should support web app + extension + python backend with consistent playback controls and provider switching.',
                    'type': 'txt',
                    'progress': 98,
                    'date': 'Apr 8, 2025',
                    'category': 'imported',
                    'folder_id': 'f1',
                    'source': 'Text import',
                    'created_at': '2025-04-08T09:00:00Z',
                },
                {
                    'id': 'podcast-1',
                    'title': 'Weekly AI Reading Digest',
                    'content': 'Podcast script generated from this week imported docs.\n\nSection 1: model ecosystem.\nSection 2: toolchain updates.\nSection 3: open-source opportunities for Readio.',
                    'type': 'txt',
                    'progress': 12,
                    'date': 'Feb 8, 2026',
                    'category': 'podcasts',
                    'folder_id': 'f1',
                    'source': 'Podcast workspace',
                    'created_at': '2026-02-08T09:00:00Z',
                },
            ]

            for item in defaults:
                connection.execute(
                    '''
                    INSERT INTO library_items (
                        id, title, content, type, progress, date, category, folder_id, source, created_at
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ''',
                    (
                        item['id'],
                        item['title'],
                        item['content'],
                        item['type'],
                        item['progress'],
                        item['date'],
                        item['category'],
                        item['folder_id'],
                        item['source'],
                        item['created_at'],
                    ),
                )
            connection.commit()

    def list_items(
        self,
        *,
        category: str | None = None,
        search: str | None = None,
        folder_id: str | None = None,
        item_type: str | None = None,
        progress_min: int | None = None,
        progress_max: int | None = None,
        page: int = 1,
        page_size: int = 20,
        sort_by: str = 'created_at',
        sort_order: str = 'desc',
    ) -> tuple[list[LibraryItemSummary], int, int]:
        current_page = max(1, int(page))
        current_page_size = min(100, max(1, int(page_size)))

        where_clauses = ['1=1']
        params: list[str] = []

        if category:
            where_clauses.append('category = ?')
            params.append(category)
        if folder_id:
            where_clauses.append('folder_id = ?')
            params.append(folder_id)
        if item_type:
            where_clauses.append('type = ?')
            params.append(item_type)
        if progress_min is not None:
            where_clauses.append('progress >= ?')
            params.append(str(progress_min))
        if progress_max is not None:
            where_clauses.append('progress <= ?')
            params.append(str(progress_max))
        if search:
            where_clauses.append('lower(title) LIKE ?')
            params.append(f"%{search.lower()}%")

        allowed_sort_columns = {
            'created_at': 'created_at',
            'title': 'title COLLATE NOCASE',
            'progress': 'progress',
            'date': 'created_at',
        }
        order_column = allowed_sort_columns.get(sort_by, 'created_at')
        order_direction = 'ASC' if sort_order.lower() == 'asc' else 'DESC'

        where_sql = ' AND '.join(where_clauses)
        offset = (current_page - 1) * current_page_size

        count_query = f'SELECT COUNT(*) AS count FROM library_items WHERE {where_sql}'
        # 列表接口不需要 content 字段，避免传输大量数据（如 base64 封面）
        data_query = f'''
            SELECT id, title, type, progress, date, category, folder_id, source, file_path, cover_image, voice, speed, content_format
            FROM library_items
            WHERE {where_sql}
            ORDER BY {order_column} {order_direction}
            LIMIT ? OFFSET ?
        '''

        with self._connect() as connection:
            total = int(connection.execute(count_query, params).fetchone()['count'])
            rows = connection.execute(data_query, [*params, current_page_size, offset]).fetchall()

        total_pages = max(1, math.ceil(total / current_page_size))
        return [LibraryItemSummary(**dict(row)) for row in rows], total, total_pages

    def get_item(self, item_id: str) -> LibraryItem | None:
        with self._connect() as connection:
            row = connection.execute(
                '''
                SELECT id, title, content, type, progress, date, category, folder_id, source, file_path, cover_image, voice, speed, current_chapter, content_format
                FROM library_items
                WHERE id = ?
                ''',
                (item_id,),
            ).fetchone()

        if row is None:
            return None
        return LibraryItem(**dict(row))

    def create_item(self, payload: CreateLibraryItemRequest) -> LibraryItem:
        now = datetime.now(UTC)
        item = LibraryItem(
            id=f'doc-{uuid4().hex[:12]}',
            title=payload.title.strip(),
            content=payload.content,
            type=payload.type,
            progress=payload.progress,
            date=payload.date or _format_library_date(now),
            category=payload.category,
            folder_id=payload.folder_id,
            source=payload.source.strip(),
            file_path=payload.file_path,
            cover_image=payload.cover_image,
            voice=None,
            speed=None,
            current_chapter=None,
            content_format=payload.content_format,
        )

        with self._connect() as connection:
            connection.execute(
                '''
                INSERT INTO library_items (
                    id, title, content, type, progress, date, category, folder_id, source, created_at, file_path, cover_image, voice, speed, current_chapter, content_format
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ''',
                (
                    item.id,
                    item.title,
                    item.content,
                    item.type,
                    item.progress,
                    item.date,
                    item.category,
                    item.folder_id,
                    item.source,
                    now.isoformat() + 'Z',
                    item.file_path,
                    item.cover_image,
                    item.voice,
                    item.speed,
                    item.current_chapter,
                    item.content_format,
                ),
            )
            connection.commit()

        return item

    def update_progress(self, item_id: str, progress: int) -> bool:
        clamped = max(0, min(100, progress))
        with self._connect() as connection:
            cursor = connection.execute(
                'UPDATE library_items SET progress = ? WHERE id = ?',
                (clamped, item_id),
            )
            connection.commit()
            return cursor.rowcount > 0

    def update_voice(self, item_id: str, voice: str | None) -> bool:
        """Persist the user's per-item TTS voice selection (None = use system default)."""
        with self._connect() as connection:
            cursor = connection.execute(
                'UPDATE library_items SET voice = ? WHERE id = ?',
                (voice, item_id),
            )
            connection.commit()
            return cursor.rowcount > 0

    def update_speed(self, item_id: str, speed: float | None) -> bool:
        """Persist the user's per-item TTS speed preference (None = use global default)."""
        with self._connect() as connection:
            cursor = connection.execute(
                'UPDATE library_items SET speed = ? WHERE id = ?',
                (speed, item_id),
            )
            connection.commit()
            return cursor.rowcount > 0

    def update_chapter(self, item_id: str, chapter_id: str | None) -> bool:
        """Persist the current EPUB chapter position for reading progress restoration."""
        with self._connect() as connection:
            cursor = connection.execute(
                'UPDATE library_items SET current_chapter = ? WHERE id = ?',
                (chapter_id, item_id),
            )
            connection.commit()
            return cursor.rowcount > 0

    def delete_item(self, item_id: str) -> bool:
        with self._connect() as connection:
            cursor = connection.execute(
                '''
                DELETE FROM library_items
                WHERE id = ?
                ''',
                (item_id,),
            )
            connection.commit()
            return cursor.rowcount > 0

    # ── Settings persistence ──────────────────────────────────────────────

    def get_settings(self) -> dict[str, str]:
        """Return all user settings as a flat {key: value} dict."""
        with self._connect() as connection:
            rows = connection.execute('SELECT key, value FROM settings').fetchall()
        return {row['key']: row['value'] for row in rows}

    def update_settings(self, data: dict[str, str | None]) -> None:
        """Upsert each key; a value of None deletes the key."""
        with self._connect() as connection:
            for key, value in data.items():
                if value is None:
                    connection.execute('DELETE FROM settings WHERE key = ?', (key,))
                else:
                    connection.execute(
                        '''
                        INSERT INTO settings (key, value) VALUES (?, ?)
                        ON CONFLICT(key) DO UPDATE SET value = excluded.value
                        ''',
                        (key, value),
                    )
            connection.commit()

    # ── Per-user settings persistence ────────────────────────────────────

    def get_user_settings(self, user_id: str) -> dict[str, str]:
        """Return all settings for a specific user."""
        with self._connect() as connection:
            rows = connection.execute(
                'SELECT key, value FROM user_settings WHERE user_id = ?',
                (user_id,),
            ).fetchall()
        return {row['key']: row['value'] for row in rows}

    def update_user_settings(self, user_id: str, data: dict[str, str | None]) -> None:
        """Upsert per-user settings; a value of None deletes the key."""
        with self._connect() as connection:
            for key, value in data.items():
                if value is None:
                    connection.execute(
                        'DELETE FROM user_settings WHERE user_id = ? AND key = ?',
                        (user_id, key),
                    )
                else:
                    connection.execute(
                        '''
                        INSERT INTO user_settings (user_id, key, value) VALUES (?, ?, ?)
                        ON CONFLICT(user_id, key) DO UPDATE SET value = excluded.value
                        ''',
                        (user_id, key, value),
                    )
            connection.commit()

    def get_user_api_keys(self, user_id: str) -> dict[str, str]:
        """Return only TTS-related API key settings for a user."""
        api_key_keys = {
            'elevenlabs_api_key', 'elevenlabs_model_id', 'elevenlabs_voice_id',
            'minimax_api_key', 'minimax_group_id', 'minimax_model_id', 'minimax_voice_id',
        }
        all_settings = self.get_user_settings(user_id)
        return {k: v for k, v in all_settings.items() if k in api_key_keys}
