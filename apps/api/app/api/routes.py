import asyncio
import base64
import json
import logging
import re
import uuid
from typing import Any

logger = logging.getLogger(__name__)

from fastapi import APIRouter, File, Form, HTTPException, Query, Response, UploadFile
from fastapi.responses import FileResponse
from pydantic import BaseModel
from starlette.responses import StreamingResponse

from app.core.settings import settings
from app.library.files import delete_file, save_file, get_file_path
from app.library.importers import extract_uploaded_document, fetch_url_document
from app.library.models import (
    CreateLibraryItemRequest,
    ImportUrlRequest,
    LibraryItem,
    LibraryItemsResponse,
)
from app.library.store import LibraryStore
from app.tts.jobs import TTSJobManager
from app.tts.models import (
    SynthesizeResponse,
    TTSSessionCancelRequest,
    TTSSessionCancelResponse,
    TTSJobCreateRequest,
    TTSJobStatusResponse,
    TTSRequest,
)
from app.tts.providers.edge import EdgeTTSProvider
from app.tts.providers.minimax import DEFAULT_MINIMAX_VOICE, fetch_system_voices
from app.tts.registry import build_tts_router
from app.api.text_utils import contains_cjk, split_sentences, split_cjk_clauses


router = APIRouter()
tts_router = build_tts_router()
tts_job_manager = TTSJobManager(
    tts_router,
    max_parallel=settings.tts_job_parallel_limit,
    cache_size=settings.tts_job_cache_size,
)
library_store = LibraryStore(settings.library_db_path)


class ProviderItem(BaseModel):
    id: str
    available: bool


class ProviderStatusResponse(BaseModel):
    tts: list[ProviderItem]


def _sse_event(event: str, data: dict[str, Any]) -> str:
    return f'event: {event}\ndata: {json.dumps(data, ensure_ascii=False)}\n\n'


def _split_text_for_stream(text: str, max_chars: int) -> list[str]:
    normalized = text.strip()
    if not normalized:
        return []

    # CJK-aware sentence splitting (handles 。！？ without trailing whitespace)
    base_chunks = split_sentences(normalized)

    out: list[str] = []
    for chunk in base_chunks:
        if len(chunk) <= max_chars:
            out.append(chunk)
            continue

        # CJK text: split on clause-level punctuation (，、；：) instead of
        # hard character slicing which would break mid-sentence
        if contains_cjk(chunk):
            out.extend(split_cjk_clauses(chunk, max_chars))
            continue

        # Latin/other text: split on word boundaries
        words = chunk.split()
        if len(words) <= 1:
            out.extend(chunk[i : i + max_chars] for i in range(0, len(chunk), max_chars))
            continue

        current = ''
        for word in words:
            candidate = f'{current} {word}'.strip()
            if len(candidate) <= max_chars:
                current = candidate
            else:
                if current:
                    out.append(current)
                current = word
        if current:
            out.append(current)

    return out


def _resolve_minimax_stream_provider() -> Any | None:
    providers = getattr(tts_router, 'providers', None)
    if not isinstance(providers, dict):
        return None

    minimax_provider = providers.get('minimax')
    if minimax_provider is None:
        return None

    stream_method = getattr(minimax_provider, 'stream_synthesize', None)
    if not callable(stream_method):
        return None
    return minimax_provider


async def _read_upload_with_size_limit(
    file: UploadFile,
    *,
    max_bytes: int,
    chunk_size: int = 1024 * 1024,
) -> bytes:
    if max_bytes <= 0:
        return await file.read()

    chunks: list[bytes] = []
    total = 0
    while True:
        chunk = await file.read(chunk_size)
        if not chunk:
            break
        total += len(chunk)
        if total > max_bytes:
            raise HTTPException(status_code=413, detail='file is too large, please upload a smaller file')
        chunks.append(chunk)
    return b''.join(chunks)


@router.get('/health')
async def health() -> dict[str, str]:
    return {'status': 'ok'}


@router.get('/api/providers', response_model=ProviderStatusResponse)
async def list_providers() -> ProviderStatusResponse:
    # Keep endpoint explicit so web/extension can decide UI without probing secrets.
    providers = [
        ProviderItem(id='edge', available=EdgeTTSProvider().is_available()),
        ProviderItem(id='elevenlabs', available=bool(settings.elevenlabs_api_key)),
        ProviderItem(id='minimax', available=bool(settings.minimax_api_key)),
    ]
    return ProviderStatusResponse(tts=providers)


@router.get('/api/library/items', response_model=LibraryItemsResponse)
async def list_library_items(
    category: str | None = None,
    search: str | None = None,
    folder_id: str | None = None,
    item_type: str | None = Query(default=None, alias='type'),
    progress_min: int | None = Query(default=None),
    progress_max: int | None = Query(default=None),
    page: int = Query(default=1, ge=1),
    page_size: int = Query(default=20, ge=1, le=100),
    sort_by: str = Query(default='created_at'),
    sort_order: str = Query(default='desc'),
) -> LibraryItemsResponse:
    items, total, total_pages = await asyncio.to_thread(
        library_store.list_items,
        category=category,
        search=search,
        folder_id=folder_id,
        item_type=item_type,
        progress_min=progress_min,
        progress_max=progress_max,
        page=page,
        page_size=page_size,
        sort_by=sort_by,
        sort_order=sort_order,
    )
    return LibraryItemsResponse(
        items=items,
        total=total,
        page=page,
        page_size=page_size,
        total_pages=total_pages,
    )


@router.get('/api/library/items/{item_id}', response_model=LibraryItem)
async def get_library_item(item_id: str) -> LibraryItem:
    item = await asyncio.to_thread(library_store.get_item, item_id)
    if item is None:
        raise HTTPException(status_code=404, detail='item not found')
    return item


@router.post('/api/library/items', response_model=LibraryItem, status_code=201)
async def create_library_item(payload: CreateLibraryItemRequest) -> LibraryItem:
    return await asyncio.to_thread(library_store.create_item, payload)


@router.delete('/api/library/items/{item_id}', status_code=204)
async def delete_library_item(item_id: str) -> Response:
    item = await asyncio.to_thread(library_store.get_item, item_id)
    if item is None:
        raise HTTPException(status_code=404, detail='item not found')

    await asyncio.to_thread(library_store.delete_item, item_id)

    if item.file_path:
        await asyncio.to_thread(delete_file, item.file_path)

    return Response(status_code=204)


@router.get('/api/library/items/{item_id}/file')
async def get_library_item_file(item_id: str) -> FileResponse:
    item = await asyncio.to_thread(library_store.get_item, item_id)
    if item is None:
        raise HTTPException(status_code=404, detail='item not found')
    if not item.file_path:
        raise HTTPException(status_code=404, detail='no file associated with this item')

    abs_path = get_file_path(item.file_path)
    if not abs_path.is_file():
        raise HTTPException(status_code=404, detail='file not found on disk')

    return FileResponse(
        path=str(abs_path),
        filename=abs_path.name,
        media_type='application/octet-stream',
    )


class ProgressUpdateRequest(BaseModel):
    progress: int


@router.patch('/api/library/items/{item_id}/progress')
async def update_library_item_progress(item_id: str, payload: ProgressUpdateRequest) -> dict[str, str]:
    updated = await asyncio.to_thread(library_store.update_progress, item_id, payload.progress)
    if not updated:
        raise HTTPException(status_code=404, detail='item not found')
    return {'status': 'ok'}


class VoiceUpdateRequest(BaseModel):
    voice: str | None = None


@router.patch('/api/library/items/{item_id}/voice')
async def update_library_item_voice(item_id: str, payload: VoiceUpdateRequest) -> dict[str, str]:
    updated = await asyncio.to_thread(library_store.update_voice, item_id, payload.voice)
    if not updated:
        raise HTTPException(status_code=404, detail='item not found')
    return {'status': 'ok'}


# ---------------------------------------------------------------------------
# TTS voice catalogue — dynamically fetched from MiniMax API
# ---------------------------------------------------------------------------

class VoiceInfo(BaseModel):
    voice_id: str
    label: str
    language: str       # "en" | "zh" | "other"
    gender: str | None  # "male" | "female" | None
    description: str | None


class VoiceListResponse(BaseModel):
    voices: list[VoiceInfo]
    default_voice_id: str


# Keywords in voice_id that imply female gender
_FEMALE_KEYWORDS = {'girl', 'woman', 'lady', 'female', 'bestie', 'gentle_woman'}
# Keywords in voice_id that imply male gender
_MALE_KEYWORDS = {'man', 'boy', 'guy', 'male'}


def _parse_language(voice_id: str) -> str:
    """Infer language from MiniMax voice_id prefix convention."""
    vid_lower = voice_id.lower()
    if vid_lower.startswith('english_'):
        return 'en'
    if vid_lower.startswith('chinese'):
        return 'zh'
    return 'other'


def _parse_gender(voice_id: str) -> str | None:
    """Infer gender from keywords in the voice_id."""
    # Split on underscores and check individual tokens against keyword sets
    tokens = {t.lower() for t in voice_id.replace('(', '_').replace(')', '_').split('_') if t}
    if tokens & _FEMALE_KEYWORDS:
        return 'female'
    if tokens & _MALE_KEYWORDS:
        return 'male'
    return None


def _build_label(voice_id: str, voice_name: str | None) -> str:
    """Build a human-readable label from voice metadata.

    Prefer voice_name if available; otherwise derive from voice_id by
    stripping the language prefix and replacing underscores with spaces.
    """
    if voice_name:
        return voice_name

    # Strip language prefix (e.g. "English_" or "Chinese (Mandarin)_")
    label = re.sub(r'^(English|Chinese\s*\([^)]*\))_', '', voice_id)
    return label.replace('_', ' ')


def _convert_system_voice(raw: dict) -> VoiceInfo:
    """Convert a raw MiniMax system voice dict into our VoiceInfo model."""
    voice_id = raw.get('voice_id', '')
    voice_name = raw.get('voice_name') or None
    desc_list = raw.get('description', [])

    # MiniMax description field is a list of strings; join into one line
    description = '; '.join(desc_list) if isinstance(desc_list, list) and desc_list else None

    return VoiceInfo(
        voice_id=voice_id,
        label=_build_label(voice_id, voice_name),
        language=_parse_language(voice_id),
        gender=_parse_gender(voice_id),
        description=description,
    )


@router.get('/api/tts/voices', response_model=VoiceListResponse)
async def list_voices() -> VoiceListResponse:
    raw_voices = await fetch_system_voices()

    # Convert and filter to English + Chinese only (primary reading languages)
    voices: list[VoiceInfo] = []
    for raw in raw_voices:
        info = _convert_system_voice(raw)
        if info.language in ('en', 'zh'):
            voices.append(info)

    # Sort: English first, then Chinese, alphabetical within each group
    lang_order = {'en': 0, 'zh': 1}
    voices.sort(key=lambda v: (lang_order.get(v.language, 99), v.label))

    return VoiceListResponse(
        voices=voices,
        default_voice_id=settings.minimax_voice_id or DEFAULT_MINIMAX_VOICE,
    )


@router.post('/api/library/import/url', response_model=LibraryItem, status_code=201)
async def import_library_url(payload: ImportUrlRequest) -> LibraryItem:
    logger.info('importing url=%s', payload.url)
    try:
        document = await fetch_url_document(str(payload.url))
    except ValueError as exc:
        logger.warning('url import validation error: url=%s — %s', payload.url, exc)
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    except Exception as exc:  # noqa: BLE001
        logger.exception('url import failed: url=%s', payload.url)
        exc_type = type(exc).__name__
        raise HTTPException(status_code=502, detail=f'failed to fetch url ({exc_type}): {exc}') from exc

    title = (payload.title or document.get('title') or str(payload.url)).strip()
    content = document.get('content', '').strip()
    if not content:
        raise HTTPException(status_code=400, detail='no readable content extracted')

    created = library_store.create_item(
        CreateLibraryItemRequest(
            title=title,
            content=content,
            type='web',
            category=payload.category,
            folder_id=payload.folder_id,
            source=str(payload.url),
        )
    )
    return created


@router.post('/api/library/import/file', response_model=LibraryItem, status_code=201)
async def import_library_file(
    file: UploadFile = File(...),
    category: str = Form('imported'),
    folder_id: str = Form('f1'),
    title: str | None = Form(None),
    cover_image: str | None = Form(None),
) -> LibraryItem:
    filename = file.filename or 'upload.txt'
    try:
        raw = await _read_upload_with_size_limit(
            file,
            max_bytes=settings.library_import_max_bytes,
        )
        logger.info('importing file=%s size=%d bytes', filename, len(raw))
        document = await asyncio.wait_for(
            asyncio.to_thread(extract_uploaded_document, filename, raw),
            timeout=settings.library_import_timeout_seconds,
        )
    except TimeoutError as exc:
        logger.warning('import timed out: file=%s size=%s', filename, 'unknown')
        raise HTTPException(
            status_code=504,
            detail=f'file parsing timed out after {settings.library_import_timeout_seconds}s — try a smaller file',
        ) from exc
    except ValueError as exc:
        logger.warning('import validation error: file=%s — %s', filename, exc)
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    except HTTPException:
        raise
    except Exception as exc:  # noqa: BLE001
        # Log the full traceback so we can actually debug server-side
        logger.exception('import failed: file=%s', filename)
        exc_type = type(exc).__name__
        raise HTTPException(
            status_code=500,
            detail=f'failed to parse "{filename}" ({exc_type}): {exc}',
        ) from exc
    finally:
        await file.close()

    item_title = (title or document['title']).strip()
    doc_type = document['type']

    # For epub/pdf, save original file for frontend rendering
    file_path = None
    if doc_type in ('epub', 'pdf'):
        # Generate item ID early so we can save the file first
        from uuid import uuid4
        item_id = f'doc-{uuid4().hex[:12]}'
        file_path = save_file(item_id, filename, raw)

    # Prefer the frontend-provided cover (extracted via epub.js, no size limit)
    # over the backend's ZipFile-based extraction which is size-limited.
    resolved_cover = cover_image or document.get('cover_image')

    create_req = CreateLibraryItemRequest(
        title=item_title,
        content=document['content'],
        type=doc_type,
        category=category,
        folder_id=folder_id,
        source=f'upload:{filename}',
        file_path=file_path,
        cover_image=resolved_cover,
    )
    created = library_store.create_item(create_req)
    return created


@router.post('/api/tts/synthesize', response_model=SynthesizeResponse)
async def synthesize(payload: TTSRequest, provider: str | None = None) -> SynthesizeResponse:
    try:
        if provider:
            result = await tts_router.synthesize_with_provider(provider=provider, request=payload)
        else:
            result = await tts_router.synthesize_with_fallback(payload)
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    except Exception as exc:  # noqa: BLE001
        raise HTTPException(status_code=503, detail=str(exc)) from exc

    return SynthesizeResponse(
        provider=result.provider,
        audio_base64=base64.b64encode(result.audio_bytes).decode('utf-8'),
        duration_ms=result.duration_ms,
        sample_rate=result.sample_rate,
        trace_id=result.trace_id,
    )


@router.post('/api/tts/jobs', response_model=TTSJobStatusResponse, status_code=202)
async def create_tts_job(payload: TTSJobCreateRequest) -> TTSJobStatusResponse:
    try:
        return await tts_job_manager.enqueue(
            session_id=payload.session_id,
            item_id=payload.item_id,
            chapter_id=payload.chapter_id,
            request=payload.request,
            provider=payload.provider,
            priority=payload.priority,
        )
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    except Exception as exc:  # noqa: BLE001
        raise HTTPException(status_code=503, detail=str(exc)) from exc


@router.get('/api/tts/jobs/{job_id}', response_model=TTSJobStatusResponse)
async def get_tts_job(job_id: str, include_audio: bool = False) -> TTSJobStatusResponse:
    snapshot = tts_job_manager.get_job_snapshot(job_id, include_audio=include_audio)
    if snapshot is None:
        raise HTTPException(status_code=404, detail='job not found')
    return snapshot


@router.post('/api/tts/jobs/{job_id}/cancel', response_model=TTSJobStatusResponse)
async def cancel_tts_job(job_id: str) -> TTSJobStatusResponse:
    snapshot = await tts_job_manager.cancel_job(job_id)
    if snapshot is None:
        raise HTTPException(status_code=404, detail='job not found')
    return snapshot


@router.post('/api/tts/sessions/{session_id}/cancel', response_model=TTSSessionCancelResponse)
async def cancel_tts_session(
    session_id: str,
    payload: TTSSessionCancelRequest | None = None,
) -> TTSSessionCancelResponse:
    cancelled_job_ids = await tts_job_manager.cancel_session(
        session_id=session_id,
        keep_item_id=payload.keep_item_id if payload else None,
        keep_chapter_id=payload.keep_chapter_id if payload else None,
    )
    return TTSSessionCancelResponse(cancelled_job_ids=cancelled_job_ids)


@router.post('/api/tts/stream')
async def synthesize_stream(
    payload: TTSRequest,
    provider: str | None = None,
    max_chars: int = 220,
) -> StreamingResponse:
    normalized_text = payload.text.strip()
    if not normalized_text:
        raise HTTPException(status_code=400, detail='text is required')

    if max_chars < 40 or max_chars > 4000:
        raise HTTPException(status_code=400, detail='max_chars must be between 40 and 4000')

    minimax_stream_provider = _resolve_minimax_stream_provider() if provider == 'minimax' else None
    use_provider_stream = minimax_stream_provider is not None
    chunks = [] if use_provider_stream else _split_text_for_stream(normalized_text, max_chars=max_chars)

    request_id = str(uuid.uuid4())

    async def _event_stream():
        if use_provider_stream:
            yield _sse_event(
                'start',
                {
                    'request_id': request_id,
                    'chunks': None,
                    'provider': 'minimax',
                    'mode': 'upstream-stream',
                },
            )

            index = 0
            try:
                async for stream_chunk in minimax_stream_provider.stream_synthesize(
                    payload.model_copy(update={'text': normalized_text})
                ):
                    yield _sse_event(
                        'chunk',
                        {
                            'request_id': request_id,
                            'index': index,
                            'text': None,
                            'provider': 'minimax',
                            'trace_id': stream_chunk.trace_id,
                            'duration_ms': None,
                            'sample_rate': None,
                            'stream_status': stream_chunk.status,
                            'audio_base64': base64.b64encode(stream_chunk.audio_bytes).decode('utf-8'),
                        },
                    )
                    index += 1
            except Exception as exc:  # noqa: BLE001
                yield _sse_event(
                    'error',
                    {
                        'request_id': request_id,
                        'index': index,
                        'detail': str(exc),
                    },
                )
                return

            yield _sse_event('end', {'request_id': request_id, 'chunks': index, 'provider': 'minimax'})
            return

        if not chunks:
            yield _sse_event(
                'error',
                {
                    'request_id': request_id,
                    'detail': 'no chunks produced from input text',
                },
            )
            return

        yield _sse_event('start', {'request_id': request_id, 'chunks': len(chunks)})

        for index, chunk_text in enumerate(chunks):
            chunk_request = payload.model_copy(update={'text': chunk_text})
            try:
                if provider:
                    result = await tts_router.synthesize_with_provider(provider=provider, request=chunk_request)
                else:
                    result = await tts_router.synthesize_with_fallback(chunk_request)
            except Exception as exc:  # noqa: BLE001
                yield _sse_event(
                    'error',
                    {
                        'request_id': request_id,
                        'index': index,
                        'text': chunk_text,
                        'detail': str(exc),
                    },
                )
                return

            yield _sse_event(
                'chunk',
                {
                    'request_id': request_id,
                    'index': index,
                    'text': chunk_text,
                    'provider': result.provider,
                    'trace_id': result.trace_id,
                    'duration_ms': result.duration_ms,
                    'sample_rate': result.sample_rate,
                    'audio_base64': base64.b64encode(result.audio_bytes).decode('utf-8'),
                },
            )

        yield _sse_event('end', {'request_id': request_id, 'chunks': len(chunks)})

    return StreamingResponse(
        _event_stream(),
        media_type='text/event-stream',
        headers={
            'Cache-Control': 'no-cache',
            'X-Accel-Buffering': 'no',
        },
    )
