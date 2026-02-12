from __future__ import annotations

import base64
import json
import mimetypes
import posixpath
import re
from io import BytesIO
from html import unescape
from pathlib import Path
from zipfile import ZipFile
from xml.etree import ElementTree as ET

import httpx

from app.core.settings import settings


TITLE_PATTERN = re.compile(r'<title[^>]*>(.*?)</title>', flags=re.IGNORECASE | re.DOTALL)
SCRIPT_STYLE_PATTERN = re.compile(r'<(script|style)[^>]*>.*?</\1>', flags=re.IGNORECASE | re.DOTALL)
TAG_PATTERN = re.compile(r'<[^>]+>')
WHITESPACE_PATTERN = re.compile(r'\s+')
HEADING_PATTERN = re.compile(r'<h[1-3][^>]*>(.*?)</h[1-3]>', flags=re.IGNORECASE | re.DOTALL)
IMAGE_SRC_PATTERN = re.compile(r'<img[^>]+src=["\']([^"\']+)["\']', flags=re.IGNORECASE)
EPUB_FORMAT = 'readio_epub_v1'


def _build_data_url(payload: bytes, name: str) -> str:
    mime_type = mimetypes.guess_type(name)[0] or 'application/octet-stream'
    encoded = base64.b64encode(payload).decode('ascii')
    return f'data:{mime_type};base64,{encoded}'


def _build_limited_data_url(payload: bytes, name: str, *, max_bytes: int) -> str | None:
    if max_bytes > 0 and len(payload) > max_bytes:
        return None
    return _build_data_url(payload, name)


def _safe_parse_xml(payload: bytes) -> ET.Element | None:
    try:
        return ET.fromstring(payload)
    except Exception:  # noqa: BLE001
        return None


def _resolve_archive_path(base_name: str, href: str) -> str | None:
    candidate = href.strip()
    if not candidate:
        return None
    lowered = candidate.lower()
    if lowered.startswith(('http://', 'https://', 'data:', 'mailto:', '#')):
        return None

    if candidate.startswith('/'):
        resolved = candidate.lstrip('/')
    else:
        resolved = posixpath.normpath(posixpath.join(posixpath.dirname(base_name), candidate))
    return resolved.lstrip('./')


def _extract_heading(html: str) -> str:
    match = HEADING_PATTERN.search(html)
    if not match:
        return ''
    plain = TAG_PATTERN.sub(' ', match.group(1))
    return WHITESPACE_PATTERN.sub(' ', unescape(plain)).strip()


def _find_first_image_data_url(
    archive: ZipFile,
    html_name: str,
    html: str,
    name_lookup: dict[str, str],
    *,
    max_bytes: int,
) -> str | None:
    for match in IMAGE_SRC_PATTERN.finditer(html):
        href = unescape(match.group(1)).strip()
        resolved = _resolve_archive_path(html_name, href)
        if not resolved:
            continue
        canonical_name = name_lookup.get(resolved.lower(), resolved)
        try:
            payload = archive.read(canonical_name)
        except KeyError:
            continue
        data_url = _build_limited_data_url(payload, canonical_name, max_bytes=max_bytes)
        if data_url:
            return data_url
    return None


def _resolve_opf_path(archive: ZipFile, name_lookup: dict[str, str]) -> str | None:
    try:
        container_xml = archive.read('META-INF/container.xml')
    except KeyError:
        container_xml = b''

    root = _safe_parse_xml(container_xml)
    if root is not None:
        for node in root.findall('.//{*}rootfile'):
            full_path = (node.attrib.get('full-path') or '').strip()
            if not full_path:
                continue
            canonical = name_lookup.get(full_path.lower(), full_path)
            if canonical in archive.namelist():
                return canonical

    for name in archive.namelist():
        if name.lower().endswith('.opf'):
            return name
    return None


def _extract_epub_manifest(
    archive: ZipFile,
    opf_name: str | None,
    name_lookup: dict[str, str],
    *,
    max_image_bytes: int,
) -> dict[str, str | list[str]]:
    fallback_names = sorted(name for name in archive.namelist() if name.lower().endswith(('.xhtml', '.html', '.htm')))
    if not opf_name:
        return {
            'title': '',
            'author': '',
            'cover_image': '',
            'chapter_names': fallback_names,
        }

    try:
        opf_xml = archive.read(opf_name)
    except KeyError:
        return {
            'title': '',
            'author': '',
            'cover_image': '',
            'chapter_names': fallback_names,
        }

    root = _safe_parse_xml(opf_xml)
    if root is None:
        return {
            'title': '',
            'author': '',
            'cover_image': '',
            'chapter_names': fallback_names,
        }

    package_title = ''
    package_author = ''
    title_node = root.find('.//{*}metadata/{*}title')
    if title_node is not None and title_node.text:
        package_title = title_node.text.strip()
    author_node = root.find('.//{*}metadata/{*}creator')
    if author_node is not None and author_node.text:
        package_author = author_node.text.strip()

    manifest: dict[str, dict[str, str]] = {}
    for node in root.findall('.//{*}manifest/{*}item'):
        item_id = (node.attrib.get('id') or '').strip()
        href = (node.attrib.get('href') or '').strip()
        media_type = (node.attrib.get('media-type') or '').strip().lower()
        properties = (node.attrib.get('properties') or '').strip().lower()
        if not item_id or not href:
            continue
        resolved = _resolve_archive_path(opf_name, href)
        if not resolved:
            continue
        canonical = name_lookup.get(resolved.lower(), resolved)
        manifest[item_id] = {
            'name': canonical,
            'media_type': media_type,
            'properties': properties,
        }

    chapter_names: list[str] = []
    for node in root.findall('.//{*}spine/{*}itemref'):
        idref = (node.attrib.get('idref') or '').strip()
        item = manifest.get(idref)
        if not item:
            continue
        name = item['name']
        media_type = item['media_type']
        if name.lower().endswith(('.xhtml', '.html', '.htm')) or media_type in ('application/xhtml+xml', 'text/html'):
            chapter_names.append(name)

    if not chapter_names:
        chapter_names = fallback_names

    cover_image = ''
    cover_id = ''
    for node in root.findall('.//{*}metadata/{*}meta'):
        if (node.attrib.get('name') or '').strip().lower() == 'cover':
            cover_id = (node.attrib.get('content') or '').strip()
            if cover_id:
                break

    if cover_id and cover_id in manifest:
        cover_name = manifest[cover_id]['name']
        try:
            cover_image = _build_limited_data_url(
                archive.read(cover_name),
                cover_name,
                max_bytes=max_image_bytes,
            ) or ''
        except KeyError:
            cover_image = ''

    if not cover_image:
        for item in manifest.values():
            is_cover = 'cover-image' in item['properties'] or 'cover' in posixpath.basename(item['name']).lower()
            if not is_cover:
                continue
            media_type = item['media_type']
            if not media_type.startswith('image/') and not item['name'].lower().endswith(('.jpg', '.jpeg', '.png', '.webp', '.gif')):
                continue
            try:
                limited_cover = _build_limited_data_url(
                    archive.read(item['name']),
                    item['name'],
                    max_bytes=max_image_bytes,
                )
                if limited_cover:
                    cover_image = limited_cover
                    break
            except KeyError:
                continue

    return {
        'title': package_title,
        'author': package_author,
        'cover_image': cover_image,
        'chapter_names': chapter_names,
    }


def _extract_title(html: str) -> str:
    match = TITLE_PATTERN.search(html)
    if not match:
        return ''
    return unescape(match.group(1)).strip()


def _extract_text(html: str) -> str:
    without_scripts = SCRIPT_STYLE_PATTERN.sub(' ', html)
    plain = TAG_PATTERN.sub(' ', without_scripts)
    normalized = WHITESPACE_PATTERN.sub(' ', unescape(plain)).strip()
    return normalized


def _decode_text_bytes(data: bytes) -> str:
    return data.decode('utf-8', errors='ignore').strip()


def _extract_pdf_text(data: bytes) -> str:
    try:
        import pymupdf
    except ImportError as exc:
        raise ValueError('PDF support requires pymupdf; install with: pip install pymupdf') from exc

    try:
        doc = pymupdf.open(stream=data, filetype='pdf')
    except Exception as exc:
        raise ValueError(f'failed to parse PDF: {exc}') from exc
    pages = []
    for page in doc:
        text = page.get_text('text')
        if text.strip():
            pages.append(text.strip())
    doc.close()
    return '\n\n'.join(pages) if pages else ''


def _extract_docx_text(data: bytes) -> str:
    with ZipFile(BytesIO(data)) as archive:
        try:
            xml = archive.read('word/document.xml')
        except KeyError as exc:
            raise ValueError('invalid docx file') from exc
    plain = TAG_PATTERN.sub(' ', xml.decode('utf-8', errors='ignore'))
    return WHITESPACE_PATTERN.sub(' ', unescape(plain)).strip()


def _extract_epub_text(data: bytes) -> str:
    with ZipFile(BytesIO(data)) as archive:
        name_lookup = {name.lower(): name for name in archive.namelist()}
        opf_name = _resolve_opf_path(archive, name_lookup)
        manifest = _extract_epub_manifest(
            archive,
            opf_name,
            name_lookup,
            max_image_bytes=settings.library_embedded_image_max_bytes,
        )

        chapter_entries: list[dict[str, str | None]] = []
        total_chars = 0
        for name in manifest['chapter_names']:
            canonical_name = name_lookup.get(str(name).lower(), str(name))
            try:
                html = archive.read(canonical_name).decode('utf-8', errors='ignore')
            except Exception:  # noqa: BLE001
                continue

            text = _extract_text(html)
            if not text:
                continue

            max_chars = settings.library_epub_max_chars
            if max_chars > 0:
                remaining = max_chars - total_chars
                if remaining <= 0:
                    break
                if len(text) > remaining:
                    text = text[:remaining]
                total_chars += len(text)

            chapter_title = _extract_heading(html) or _extract_title(html) or f'Chapter {len(chapter_entries) + 1}'
            chapter_image = None
            if settings.library_epub_chapter_image_limit <= 0 or len(chapter_entries) < settings.library_epub_chapter_image_limit:
                chapter_image = _find_first_image_data_url(
                    archive,
                    canonical_name,
                    html,
                    name_lookup,
                    max_bytes=settings.library_embedded_image_max_bytes,
                )
            chapter_entries.append(
                {
                    'id': f'chapter-{len(chapter_entries) + 1}',
                    'title': chapter_title,
                    'text': text,
                    'image': chapter_image,
                }
            )

    if not chapter_entries:
        return ''

    first_cover = next((entry['image'] for entry in chapter_entries if entry.get('image')), None)
    payload = {
        'format': EPUB_FORMAT,
        'title': (manifest['title'] or '').strip(),
        'author': (manifest['author'] or '').strip() or None,
        'cover_image': manifest['cover_image'] or first_cover,
        'chapters': chapter_entries,
    }
    return json.dumps(payload, ensure_ascii=False)


def extract_uploaded_document(filename: str, data: bytes) -> dict[str, str]:
    suffix = Path(filename).suffix.lower()
    stem = Path(filename).stem.strip() or 'Untitled'

    if suffix == '.txt':
        text = _decode_text_bytes(data)
        doc_type = 'txt'
    elif suffix == '.pdf':
        text = _extract_pdf_text(data)
        doc_type = 'pdf'
    elif suffix == '.docx':
        text = _extract_docx_text(data)
        doc_type = 'txt'
    elif suffix == '.epub':
        text = _extract_epub_text(data)
        doc_type = 'epub'
        try:
            epub_payload = json.loads(text)
            epub_title = epub_payload.get('title')
            if isinstance(epub_title, str) and epub_title.strip():
                stem = epub_title.strip()
        except Exception:  # noqa: BLE001
            pass
    else:
        raise ValueError('unsupported file type, expected txt/pdf/docx/epub')

    text = text.strip()
    if not text:
        raise ValueError('no readable content extracted')

    return {
        'title': stem,
        'content': text,
        'type': doc_type,
    }


async def fetch_url_document(url: str) -> dict[str, str]:
    headers = {
        'User-Agent': 'ReadioBot/0.1 (+https://github.com/readio/readio)',
        'Accept': 'text/html,application/xhtml+xml',
    }
    async with httpx.AsyncClient(timeout=20.0, follow_redirects=True) as client:
        response = await client.get(url, headers=headers)
        response.raise_for_status()
        content_type = response.headers.get('content-type', '')
        if 'text/html' not in content_type and 'application/xhtml+xml' not in content_type:
            raise ValueError('url is not an HTML page')

        html = response.text

    text = _extract_text(html)
    if not text:
        raise ValueError('no readable content extracted')

    title = _extract_title(html)
    return {
        'title': title,
        'content': text,
    }
