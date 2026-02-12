from __future__ import annotations

import asyncio
import base64
import hashlib
import json
import time
import uuid
from collections import OrderedDict
from dataclasses import dataclass, field
from typing import Literal, Protocol

from app.tts.models import TTSJobPriority, TTSJobStatusResponse, TTSRequest, TTSResult


class _RouterProtocol(Protocol):
    async def synthesize_with_fallback(self, request: TTSRequest) -> TTSResult: ...

    async def synthesize_with_provider(self, provider: str, request: TTSRequest) -> TTSResult: ...


JobStatus = Literal['queued', 'running', 'completed', 'failed', 'cancelled']


@dataclass
class _TTSJobRecord:
    job_id: str
    session_id: str
    item_id: str
    chapter_id: str
    priority: TTSJobPriority
    provider: str | None
    request: TTSRequest
    cache_key: str
    status: JobStatus = 'queued'
    created_at: float = field(default_factory=time.time)
    started_at: float | None = None
    finished_at: float | None = None
    error: str | None = None
    result: TTSResult | None = None
    cache_hit: bool = False
    cancel_requested: bool = False
    done_event: asyncio.Event = field(default_factory=asyncio.Event)


def _priority_value(priority: TTSJobPriority) -> int:
    return 0 if priority == 'user' else 1


def _timestamp_ms(value: float | None) -> int | None:
    if value is None:
        return None
    return int(value * 1000)


class TTSJobManager:
    def __init__(self, router: _RouterProtocol, *, max_parallel: int = 2, cache_size: int = 128) -> None:
        self._router = router
        self._max_parallel = max(1, max_parallel)
        self._cache_size = max(0, cache_size)

        self._queue: asyncio.PriorityQueue[tuple[int, int, str]] | None = None
        self._jobs: dict[str, _TTSJobRecord] = {}
        self._running_tasks: dict[str, asyncio.Task[TTSResult]] = {}
        self._workers: list[asyncio.Task[None]] = []
        self._cache: OrderedDict[str, TTSResult] = OrderedDict()
        self._sequence = 0
        self._loop: asyncio.AbstractEventLoop | None = None

    async def enqueue(
        self,
        *,
        session_id: str,
        item_id: str,
        chapter_id: str,
        request: TTSRequest,
        provider: str | None,
        priority: TTSJobPriority,
    ) -> TTSJobStatusResponse:
        await self._ensure_workers()

        cache_key = self._build_cache_key(provider=provider, request=request)
        cached_result = self._get_cache(cache_key)
        if cached_result is not None:
            record = _TTSJobRecord(
                job_id=str(uuid.uuid4()),
                session_id=session_id,
                item_id=item_id,
                chapter_id=chapter_id,
                priority=priority,
                provider=provider,
                request=request,
                cache_key=cache_key,
                status='completed',
                started_at=time.time(),
                finished_at=time.time(),
                result=cached_result,
                cache_hit=True,
            )
            record.done_event.set()
            self._jobs[record.job_id] = record
            return self._snapshot(record, include_audio=False)

        record = _TTSJobRecord(
            job_id=str(uuid.uuid4()),
            session_id=session_id,
            item_id=item_id,
            chapter_id=chapter_id,
            priority=priority,
            provider=provider,
            request=request,
            cache_key=cache_key,
        )
        self._jobs[record.job_id] = record

        self._sequence += 1
        if self._queue is None:
            raise RuntimeError('job queue is unavailable')
        await self._queue.put((_priority_value(priority), self._sequence, record.job_id))
        return self._snapshot(record, include_audio=False)

    def get_job_snapshot(self, job_id: str, *, include_audio: bool) -> TTSJobStatusResponse | None:
        record = self._jobs.get(job_id)
        if record is None:
            return None
        return self._snapshot(record, include_audio=include_audio)

    async def cancel_job(self, job_id: str) -> TTSJobStatusResponse | None:
        record = self._jobs.get(job_id)
        if record is None:
            return None

        if record.status in ('completed', 'failed', 'cancelled'):
            return self._snapshot(record, include_audio=False)

        record.cancel_requested = True
        if record.status == 'queued':
            self._mark_cancelled(record)
            return self._snapshot(record, include_audio=False)

        running_task = self._running_tasks.get(job_id)
        if running_task and not running_task.done():
            running_task.cancel()

        return self._snapshot(record, include_audio=False)

    async def cancel_session(
        self,
        session_id: str,
        *,
        keep_item_id: str | None = None,
        keep_chapter_id: str | None = None,
    ) -> list[str]:
        cancelled: list[str] = []
        for job_id, record in list(self._jobs.items()):
            if record.session_id != session_id:
                continue
            if record.status not in ('queued', 'running'):
                continue

            keep_current_context = (
                keep_item_id is not None
                and keep_chapter_id is not None
                and record.item_id == keep_item_id
                and record.chapter_id == keep_chapter_id
            )
            if keep_current_context:
                continue

            snapshot = await self.cancel_job(job_id)
            if snapshot is not None:
                cancelled.append(job_id)
        return cancelled

    async def shutdown(self) -> None:
        for task in list(self._running_tasks.values()):
            if not task.done():
                task.cancel()

        for worker in list(self._workers):
            worker.cancel()

        if self._workers:
            await asyncio.gather(*self._workers, return_exceptions=True)

        self._running_tasks.clear()
        self._workers.clear()
        self._loop = None
        self._queue = None

    async def _ensure_workers(self) -> None:
        loop = asyncio.get_running_loop()
        if self._loop is None:
            self._loop = loop
            self._queue = asyncio.PriorityQueue()
        elif self._loop is not loop:
            # Requests can run in different loops across test clients; reset worker state for the new loop.
            self._workers = []
            self._running_tasks = {}
            self._queue = asyncio.PriorityQueue()
            self._loop = loop

        self._workers = [worker for worker in self._workers if not worker.done()]
        while len(self._workers) < self._max_parallel:
            self._workers.append(asyncio.create_task(self._worker_loop()))

    async def _worker_loop(self) -> None:
        while True:
            queue = self._queue
            if queue is None:
                await asyncio.sleep(0)
                continue
            _, _, job_id = await queue.get()
            try:
                record = self._jobs.get(job_id)
                if record is None:
                    continue

                if record.status != 'queued':
                    if record.status == 'cancelled':
                        record.done_event.set()
                    continue

                if record.cancel_requested:
                    self._mark_cancelled(record)
                    continue

                record.status = 'running'
                record.started_at = time.time()

                task = asyncio.create_task(self._execute(record))
                self._running_tasks[record.job_id] = task

                try:
                    result = await task
                except asyncio.CancelledError:
                    self._mark_cancelled(record)
                except Exception as exc:  # noqa: BLE001
                    if record.cancel_requested:
                        self._mark_cancelled(record)
                    else:
                        detail = str(exc).strip() or exc.__class__.__name__
                        record.status = 'failed'
                        record.error = detail
                        record.finished_at = time.time()
                        record.done_event.set()
                else:
                    if record.cancel_requested:
                        self._mark_cancelled(record)
                    else:
                        record.status = 'completed'
                        record.result = result
                        record.finished_at = time.time()
                        self._remember_cache(record.cache_key, result)
                        record.done_event.set()
                finally:
                    self._running_tasks.pop(record.job_id, None)
            finally:
                queue.task_done()

    async def _execute(self, record: _TTSJobRecord) -> TTSResult:
        if record.provider:
            return await self._router.synthesize_with_provider(provider=record.provider, request=record.request)
        return await self._router.synthesize_with_fallback(record.request)

    def _snapshot(self, record: _TTSJobRecord, *, include_audio: bool) -> TTSJobStatusResponse:
        result = record.result
        return TTSJobStatusResponse(
            job_id=record.job_id,
            session_id=record.session_id,
            item_id=record.item_id,
            chapter_id=record.chapter_id,
            priority=record.priority,
            status=record.status,
            provider=result.provider if result else record.provider,
            trace_id=result.trace_id if result else None,
            duration_ms=result.duration_ms if result else None,
            sample_rate=result.sample_rate if result else None,
            error=record.error,
            cache_hit=record.cache_hit,
            created_at_ms=_timestamp_ms(record.created_at) or 0,
            started_at_ms=_timestamp_ms(record.started_at),
            finished_at_ms=_timestamp_ms(record.finished_at),
            audio_base64=(
                base64.b64encode(result.audio_bytes).decode('utf-8')
                if include_audio and result is not None and record.status == 'completed'
                else None
            ),
        )

    def _mark_cancelled(self, record: _TTSJobRecord) -> None:
        record.status = 'cancelled'
        record.finished_at = time.time()
        record.done_event.set()

    def _build_cache_key(self, *, provider: str | None, request: TTSRequest) -> str:
        payload = {
            'provider': provider or 'auto',
            'text': request.text,
            'voice': request.voice,
            'speed': request.speed,
            'language': request.language,
            'format': request.format,
            'provider_options': request.provider_options,
        }
        serialized = json.dumps(payload, ensure_ascii=False, sort_keys=True, default=str)
        return hashlib.sha256(serialized.encode('utf-8')).hexdigest()

    def _remember_cache(self, cache_key: str, result: TTSResult) -> None:
        if self._cache_size <= 0:
            return
        self._cache[cache_key] = result
        self._cache.move_to_end(cache_key)
        while len(self._cache) > self._cache_size:
            self._cache.popitem(last=False)

    def _get_cache(self, cache_key: str) -> TTSResult | None:
        if self._cache_size <= 0:
            return None
        result = self._cache.get(cache_key)
        if result is None:
            return None
        self._cache.move_to_end(cache_key)
        return result
