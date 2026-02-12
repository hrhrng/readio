from dataclasses import dataclass
from typing import Any, Literal

from pydantic import BaseModel, Field


class TTSRequest(BaseModel):
    text: str = Field(min_length=1)
    voice: str | None = None
    speed: float | None = None
    language: str | None = None
    format: str = Field(default='mp3')
    provider_options: dict[str, Any] = Field(default_factory=dict)


@dataclass
class TTSResult:
    provider: str
    audio_bytes: bytes
    duration_ms: int | None
    sample_rate: int | None
    trace_id: str


class SynthesizeResponse(BaseModel):
    provider: str
    audio_base64: str
    duration_ms: int | None
    sample_rate: int | None
    trace_id: str


TTSJobPriority = Literal['user', 'prefetch']
TTSJobStatus = Literal['queued', 'running', 'completed', 'failed', 'cancelled']


class TTSJobCreateRequest(BaseModel):
    session_id: str = Field(min_length=1, max_length=160)
    item_id: str = Field(min_length=1, max_length=160)
    chapter_id: str = Field(min_length=1, max_length=160)
    priority: TTSJobPriority = 'user'
    provider: str | None = None
    request: TTSRequest


class TTSJobStatusResponse(BaseModel):
    job_id: str
    session_id: str
    item_id: str
    chapter_id: str
    priority: TTSJobPriority
    status: TTSJobStatus
    provider: str | None = None
    trace_id: str | None = None
    duration_ms: int | None = None
    sample_rate: int | None = None
    error: str | None = None
    cache_hit: bool = False
    created_at_ms: int
    started_at_ms: int | None = None
    finished_at_ms: int | None = None
    audio_base64: str | None = None


class TTSSessionCancelRequest(BaseModel):
    keep_item_id: str | None = None
    keep_chapter_id: str | None = None


class TTSSessionCancelResponse(BaseModel):
    cancelled_job_ids: list[str]
