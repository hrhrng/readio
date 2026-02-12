import asyncio
import base64

import pytest

from app.tts.jobs import TTSJobManager
from app.tts.models import TTSRequest, TTSResult


class SlowRouter:
    def __init__(self, delay: float = 0.05) -> None:
        self.delay = delay
        self.calls: list[str] = []
        self.active = 0
        self.max_active = 0

    async def synthesize_with_fallback(self, request: TTSRequest) -> TTSResult:
        self.calls.append(request.text)
        self.active += 1
        self.max_active = max(self.max_active, self.active)
        try:
            await asyncio.sleep(self.delay)
        finally:
            self.active -= 1
        return TTSResult(
            provider='edge',
            audio_bytes=f'audio:{request.text}'.encode('utf-8'),
            duration_ms=900,
            sample_rate=24000,
            trace_id=f'trace:{request.text}',
        )

    async def synthesize_with_provider(self, provider: str, request: TTSRequest) -> TTSResult:
        return await self.synthesize_with_fallback(request)


async def _wait_for_status(manager: TTSJobManager, job_id: str, expected: str, timeout: float = 1.0) -> None:
    deadline = asyncio.get_running_loop().time() + timeout
    while asyncio.get_running_loop().time() < deadline:
        snapshot = manager.get_job_snapshot(job_id, include_audio=False)
        if snapshot and snapshot.status == expected:
            return
        await asyncio.sleep(0.01)
    raise AssertionError(f'job {job_id} did not reach status={expected}')


@pytest.mark.asyncio
async def test_job_manager_respects_parallel_limit() -> None:
    router = SlowRouter(delay=0.08)
    manager = TTSJobManager(router, max_parallel=2, cache_size=8)

    try:
        first = await manager.enqueue(
            session_id='session-1',
            item_id='item-1',
            chapter_id='chapter-1',
            request=TTSRequest(text='one'),
            provider=None,
            priority='user',
        )
        second = await manager.enqueue(
            session_id='session-1',
            item_id='item-1',
            chapter_id='chapter-2',
            request=TTSRequest(text='two'),
            provider=None,
            priority='user',
        )
        third = await manager.enqueue(
            session_id='session-1',
            item_id='item-1',
            chapter_id='chapter-3',
            request=TTSRequest(text='three'),
            provider=None,
            priority='user',
        )

        await asyncio.sleep(0.02)

        statuses = [
            manager.get_job_snapshot(first.job_id, include_audio=False).status,
            manager.get_job_snapshot(second.job_id, include_audio=False).status,
            manager.get_job_snapshot(third.job_id, include_audio=False).status,
        ]

        assert statuses.count('running') <= 2
        assert statuses.count('queued') >= 1

        await _wait_for_status(manager, first.job_id, 'completed')
        await _wait_for_status(manager, second.job_id, 'completed')
        await _wait_for_status(manager, third.job_id, 'completed')
        assert router.max_active == 2
    finally:
        await manager.shutdown()


@pytest.mark.asyncio
async def test_job_manager_prioritizes_user_over_prefetch() -> None:
    router = SlowRouter(delay=0.06)
    manager = TTSJobManager(router, max_parallel=1, cache_size=8)

    try:
        running = await manager.enqueue(
            session_id='session-1',
            item_id='item-1',
            chapter_id='chapter-1',
            request=TTSRequest(text='prefetch-1'),
            provider=None,
            priority='prefetch',
        )
        await asyncio.sleep(0.01)

        queued_prefetch = await manager.enqueue(
            session_id='session-1',
            item_id='item-1',
            chapter_id='chapter-2',
            request=TTSRequest(text='prefetch-2'),
            provider=None,
            priority='prefetch',
        )
        queued_user = await manager.enqueue(
            session_id='session-1',
            item_id='item-1',
            chapter_id='chapter-2',
            request=TTSRequest(text='user-1'),
            provider=None,
            priority='user',
        )

        await _wait_for_status(manager, running.job_id, 'completed')
        await _wait_for_status(manager, queued_user.job_id, 'completed')
        await _wait_for_status(manager, queued_prefetch.job_id, 'completed')

        assert router.calls == ['prefetch-1', 'user-1', 'prefetch-2']
    finally:
        await manager.shutdown()


@pytest.mark.asyncio
async def test_job_manager_returns_cache_hit_for_same_synthesis() -> None:
    router = SlowRouter(delay=0.01)
    manager = TTSJobManager(router, max_parallel=1, cache_size=8)

    try:
        first = await manager.enqueue(
            session_id='session-1',
            item_id='item-1',
            chapter_id='chapter-1',
            request=TTSRequest(text='cached text'),
            provider=None,
            priority='user',
        )
        await _wait_for_status(manager, first.job_id, 'completed')
        first_snapshot = manager.get_job_snapshot(first.job_id, include_audio=True)
        assert first_snapshot is not None
        assert first_snapshot.cache_hit is False

        second = await manager.enqueue(
            session_id='session-1',
            item_id='item-2',
            chapter_id='chapter-2',
            request=TTSRequest(text='cached text'),
            provider=None,
            priority='prefetch',
        )
        second_snapshot = manager.get_job_snapshot(second.job_id, include_audio=True)

        assert second_snapshot is not None
        assert second_snapshot.status == 'completed'
        assert second_snapshot.cache_hit is True
        assert second_snapshot.audio_base64 == base64.b64encode(b'audio:cached text').decode('utf-8')
        assert router.calls == ['cached text']
    finally:
        await manager.shutdown()
