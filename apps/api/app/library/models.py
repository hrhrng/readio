from typing import Literal

from pydantic import BaseModel, Field


LibraryItemType = Literal['web', 'txt', 'pdf', 'epub']
LibraryItemCategory = Literal['imported', 'podcasts']


class LibraryItem(BaseModel):
    id: str
    title: str
    content: str
    type: LibraryItemType
    progress: int = Field(ge=0, le=100)
    date: str
    category: LibraryItemCategory
    folder_id: str
    source: str
    file_path: str | None = None
    cover_image: str | None = None
    voice: str | None = None
    speed: float | None = None
    current_chapter: str | None = None
    content_format: str | None = None


class LibraryItemSummary(BaseModel):
    """列表接口用的轻量模型，不含 content。"""
    id: str
    title: str
    type: LibraryItemType
    progress: int = Field(ge=0, le=100)
    date: str
    category: LibraryItemCategory
    folder_id: str
    source: str
    file_path: str | None = None
    cover_image: str | None = None
    voice: str | None = None
    speed: float | None = None
    content_format: str | None = None


class LibraryItemsResponse(BaseModel):
    items: list[LibraryItemSummary]
    total: int
    page: int
    page_size: int
    total_pages: int


class CreateLibraryItemRequest(BaseModel):
    title: str = Field(min_length=1, max_length=240)
    content: str = Field(min_length=1)
    type: LibraryItemType = 'txt'
    category: LibraryItemCategory = 'imported'
    folder_id: str = Field(default='f1', min_length=1)
    source: str = Field(default='Manual Input', min_length=1)
    progress: int = Field(default=0, ge=0, le=100)
    date: str | None = None
    file_path: str | None = None
    cover_image: str | None = None
    content_format: str | None = None


PositionAffinity = Literal['forward', 'backward']
PositionMutationStatus = Literal['applied', 'conflict']


class ReadingPositionMutation(BaseModel):
    document_revision: str = Field(min_length=1, max_length=200)
    char_offset: int = Field(ge=0, le=2**64 - 1)
    affinity: PositionAffinity
    observed_at_unix_ms: int = Field(ge=-(2**63), le=2**63 - 1)
    session_id: str = Field(min_length=1, max_length=200)
    mutation_id: str = Field(min_length=1, max_length=200)
    base_version: int = Field(ge=0, le=2**64 - 1)


class ReadingPositionRecord(ReadingPositionMutation):
    version: int = Field(ge=1, le=2**64 - 1)


class PositionCandidate(ReadingPositionRecord):
    origin: Literal['current', 'incoming']


class PositionMutationResponse(BaseModel):
    status: PositionMutationStatus
    idempotent_replay: bool = False
    version: int = Field(ge=0, le=2**64 - 1)
    position: ReadingPositionRecord | None = None
    conflict_id: str | None = None
    candidates: list[PositionCandidate] = Field(default_factory=list)


BookmarkMutationStatus = Literal['applied', 'conflict']


class BookmarkMutationRequest(BaseModel):
    bookmark_id: str = Field(min_length=1, max_length=200)
    char_offset: int = Field(ge=0, le=2**64 - 1)
    affinity: PositionAffinity
    note: str = Field(max_length=10_000)
    created_at_unix_ms: int = Field(ge=-(2**63), le=2**63 - 1)
    updated_at_unix_ms: int = Field(ge=-(2**63), le=2**63 - 1)
    base_version: int = Field(ge=0, le=2**64 - 1)
    mutation_id: str = Field(min_length=1, max_length=200)


class BookmarkRecord(BaseModel):
    id: str
    item_id: str
    char_offset: int = Field(ge=0, le=2**64 - 1)
    affinity: PositionAffinity
    note: str
    created_at_unix_ms: int
    updated_at_unix_ms: int
    version: int = Field(ge=1, le=2**64 - 1)


class BookmarkMutationResponse(BaseModel):
    status: BookmarkMutationStatus
    idempotent_replay: bool = False
    bookmark: BookmarkRecord | None = None
