from typing import Literal

from pydantic import BaseModel, Field, HttpUrl


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


class LibraryItemsResponse(BaseModel):
    items: list[LibraryItem]
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


class ImportUrlRequest(BaseModel):
    url: HttpUrl
    title: str | None = None
    category: LibraryItemCategory = 'imported'
    folder_id: str = Field(default='f1', min_length=1)
