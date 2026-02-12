import asyncio
import base64

import pytest

import app.api.routes as routes
from app.tts.jobs import TTSJobManager
from app.tts.models import TTSSessionCancelRequest, TTSJobCreateRequest, TTSRequest, TTSResult


class SlowRouter:
    def __init__(self, delay: float = 0.05) -> None:
        self.delay = delay

    async def synthesize_with_fallback(self, request: TTSRequest) -> TTSResult:
        await asyncio.sleep(self.delay)
        return TTSResult(
            provider='edge',
            audio_bytes=f'audio:{request.text}'.encode('utf-8'),
            duration_ms=1100,
            sample_rate=24000,
            trace_id='trace-job',
        )

    async def synthesize_with_provider(self, provider: str, request: TTSRequest) -> TTSResult:
        return await self.synthesize_with_fallback(request)


async def _wait_until_completed(manager: TTSJobManager, job_id: str) -> None:
    for _ in range(120):
        snapshot = manager.get_job_snapshot(job_id, include_audio=False)
        if snapshot is not None and snapshot.status == 'completed':
            return
        await asyncio.sleep(0.01)
    raise AssertionError(f'job {job_id} did not complete in time')


@pytest.mark.asyncio
async def test_tts_job_endpoint_returns_completed_audio(monkeypatch):
    manager = TTSJobManager(SlowRouter(delay=0.02), max_parallel=2, cache_size=8)
    monkeypatch.setattr(routes, 'tts_job_manager', manager)

    created = await routes.create_tts_job(
        TTSJobCreateRequest(
            session_id='session-a',
            item_id='item-1',
            chapter_id='chapter-1',
            priority='user',
            provider=None,
            request=TTSRequest(
                text='hello queue',
                speed=1.2,
                format='mp3',
                provider_options={},
            ),
        )
    )

    assert created.status in ('queued', 'running', 'completed')
    await _wait_until_completed(manager, created.job_id)
    snapshot = await routes.get_tts_job(created.job_id, include_audio=True)

    assert snapshot.status == 'completed'
    assert snapshot.provider == 'edge'
    assert snapshot.trace_id == 'trace-job'
    assert snapshot.audio_base64 == base64.b64encode(b'audio:hello queue').decode('utf-8')
    await manager.shutdown()


@pytest.mark.asyncio
async def test_tts_session_cancel_cancels_other_jobs(monkeypatch):
    manager = TTSJobManager(SlowRouter(delay=0.08), max_parallel=1, cache_size=8)
    monkeypatch.setattr(routes, 'tts_job_manager', manager)

    keep_job = await routes.create_tts_job(
        TTSJobCreateRequest(
            session_id='session-b',
            item_id='item-1',
            chapter_id='chapter-1',
            priority='user',
            request=TTSRequest(text='keep-running'),
        )
    )
    cancel_job = await routes.create_tts_job(
        TTSJobCreateRequest(
            session_id='session-b',
            item_id='item-1',
            chapter_id='chapter-2',
            priority='prefetch',
            request=TTSRequest(text='cancel-me'),
        )
    )

    session_cancel = await routes.cancel_tts_session(
        'session-b',
        TTSSessionCancelRequest(keep_item_id='item-1', keep_chapter_id='chapter-1'),
    )
    assert cancel_job.job_id in session_cancel.cancelled_job_ids
    assert keep_job.job_id not in session_cancel.cancelled_job_ids

    cancelled_status = await routes.get_tts_job(cancel_job.job_id, include_audio=False)
    assert cancelled_status.status == 'cancelled'

    await _wait_until_completed(manager, keep_job.job_id)
    await manager.shutdown()
