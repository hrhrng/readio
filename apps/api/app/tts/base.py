from typing import Protocol

from .models import TTSRequest, TTSResult


class TTSProvider(Protocol):
    @property
    def id(self) -> str: ...

    def is_available(self) -> bool: ...

    async def synthesize(self, request: TTSRequest) -> TTSResult: ...
