from io import BytesIO
import json
import time
from zipfile import ZipFile

from fastapi.testclient import TestClient

import app.api.routes as routes
from app.core.settings import settings
from app.main import app


client = TestClient(app)


def _build_epub_fixture() -> bytes:
    buffer = BytesIO()
    with ZipFile(buffer, 'w') as archive:
        archive.writestr(
            'META-INF/container.xml',
            '''<?xml version="1.0" encoding="UTF-8"?>
            <container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
              <rootfiles>
                <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml" />
              </rootfiles>
            </container>
            ''',
        )
        archive.writestr(
            'OEBPS/content.opf',
            '''<?xml version="1.0" encoding="UTF-8"?>
            <package xmlns="http://www.idpf.org/2007/opf" version="3.0">
              <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                <dc:title>成为乔布斯</dc:title>
                <dc:creator>布伦特·施兰德</dc:creator>
                <meta name="cover" content="cover-image" />
              </metadata>
              <manifest>
                <item id="cover-image" href="images/cover.jpg" media-type="image/jpeg" />
                <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml" />
                <item id="ch2" href="chapter2.xhtml" media-type="application/xhtml+xml" />
              </manifest>
              <spine>
                <itemref idref="ch1" />
                <itemref idref="ch2" />
              </spine>
            </package>
            ''',
        )
        archive.writestr(
            'OEBPS/chapter1.xhtml',
            '''<html xmlns="http://www.w3.org/1999/xhtml"><head><title>第一章</title></head>
            <body><h1>第一章 安拉的花园</h1><img src="images/cover.jpg" /><p>第一章正文内容。</p></body></html>''',
        )
        archive.writestr(
            'OEBPS/chapter2.xhtml',
            '''<html xmlns="http://www.w3.org/1999/xhtml"><head><title>第二章</title></head>
            <body><h1>第二章 我不想当商人</h1><p>第二章正文内容。</p></body></html>''',
        )
        archive.writestr('OEBPS/images/cover.jpg', b'\xff\xd8\xff\xe0\x00\x10JFIF')
    return buffer.getvalue()


def test_import_file_txt_creates_item():
    resp = client.post(
        '/api/library/import/file',
        data={'folder_id': 'f1', 'category': 'imported'},
        files={'file': ('from-upload.txt', BytesIO(b'hello from upload file'), 'text/plain')},
    )

    assert resp.status_code == 201
    data = resp.json()
    assert data['title'] == 'from-upload'
    assert data['type'] == 'txt'
    assert data['source'] == 'upload:from-upload.txt'
    assert 'hello from upload file' in data['content']


def test_import_file_rejects_unsupported_extension():
    resp = client.post(
        '/api/library/import/file',
        data={'folder_id': 'f1', 'category': 'imported'},
        files={'file': ('archive.bin', BytesIO(b'\x00\x01\x02'), 'application/octet-stream')},
    )

    assert resp.status_code == 400
    assert 'unsupported file type' in resp.json()['detail']


def test_import_file_epub_creates_structured_epub_item():
    resp = client.post(
        '/api/library/import/file',
        data={'folder_id': 'f1', 'category': 'imported'},
        files={'file': ('becoming.epub', BytesIO(_build_epub_fixture()), 'application/epub+zip')},
    )

    assert resp.status_code == 201
    data = resp.json()
    assert data['title'] == '成为乔布斯'
    assert data['type'] == 'epub'

    payload = json.loads(data['content'])
    assert payload['format'] == 'readio_epub_v1'
    assert payload['title'] == '成为乔布斯'
    assert payload['author'] == '布伦特·施兰德'
    assert payload['cover_image'].startswith('data:image/jpeg;base64,')
    assert len(payload['chapters']) == 2
    assert payload['chapters'][0]['title'] == '第一章 安拉的花园'
    assert '第一章正文内容。' in payload['chapters'][0]['text']


def test_import_file_returns_timeout_when_parsing_takes_too_long(monkeypatch):
    original_timeout = settings.library_import_timeout_seconds
    settings.library_import_timeout_seconds = 0.01

    def _slow_extract(filename: str, raw: bytes):  # type: ignore[unused-argument]
        time.sleep(0.05)
        return {'title': 'slow', 'content': 'slow', 'type': 'txt'}

    monkeypatch.setattr(routes, 'extract_uploaded_document', _slow_extract)

    try:
        resp = client.post(
            '/api/library/import/file',
            data={'folder_id': 'f1', 'category': 'imported'},
            files={'file': ('slow.txt', BytesIO(b'hello'), 'text/plain')},
        )
    finally:
        settings.library_import_timeout_seconds = original_timeout

    assert resp.status_code == 504
    assert 'file parsing timed out' in resp.json()['detail']


def test_import_file_rejects_payload_that_exceeds_size_limit():
    original_max_bytes = settings.library_import_max_bytes
    settings.library_import_max_bytes = 8
    try:
        resp = client.post(
            '/api/library/import/file',
            data={'folder_id': 'f1', 'category': 'imported'},
            files={'file': ('too-big.txt', BytesIO(b'0123456789'), 'text/plain')},
        )
    finally:
        settings.library_import_max_bytes = original_max_bytes

    assert resp.status_code == 413
    assert 'file is too large' in resp.json()['detail']


def test_import_file_epub_strips_chapter_images_when_limit_is_too_small():
    original_image_bytes = settings.library_embedded_image_max_bytes
    settings.library_embedded_image_max_bytes = 4
    try:
        resp = client.post(
            '/api/library/import/file',
            data={'folder_id': 'f1', 'category': 'imported'},
            files={'file': ('becoming.epub', BytesIO(_build_epub_fixture()), 'application/epub+zip')},
        )
    finally:
        settings.library_embedded_image_max_bytes = original_image_bytes

    assert resp.status_code == 201
    payload = json.loads(resp.json()['content'])
    assert payload['cover_image'] is None
    assert payload['chapters'][0]['image'] is None


def test_import_file_pdf_invalid_returns_400():
    resp = client.post(
        '/api/library/import/file',
        data={'folder_id': 'f1', 'category': 'imported'},
        files={'file': ('book.pdf', BytesIO(b'%PDF-1.5 fake'), 'application/pdf')},
    )
    assert resp.status_code == 400
    detail = resp.json()['detail'].lower()
    assert 'pdf' in detail or 'pymupdf' in detail


def test_delete_item_cleans_up_associated_file(tmp_path, monkeypatch):
    # Override files dir to tmp_path
    monkeypatch.setattr(settings, 'library_files_dir', str(tmp_path))

    # Import an epub (which saves the original file)
    resp = client.post(
        '/api/library/import/file',
        data={'folder_id': 'f1', 'category': 'imported'},
        files={'file': ('cleanup.epub', BytesIO(_build_epub_fixture()), 'application/epub+zip')},
    )
    assert resp.status_code == 201
    item_id = resp.json()['id']
    file_path = resp.json().get('file_path')
    assert file_path is not None

    # Verify file exists on disk
    from app.library.files import get_file_path
    abs_path = get_file_path(file_path)
    assert abs_path.is_file()

    # Delete the item
    del_resp = client.delete(f'/api/library/items/{item_id}')
    assert del_resp.status_code == 204

    # Verify file is cleaned up
    assert not abs_path.is_file()

    # Verify item no longer exists
    get_resp = client.get(f'/api/library/items/{item_id}')
    assert get_resp.status_code == 404


def test_delete_nonexistent_item_returns_404():
    resp = client.delete('/api/library/items/nonexistent-id')
    assert resp.status_code == 404
