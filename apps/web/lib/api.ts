import {
  LibraryItem,
  LibraryItemsResponse,
  TTSJobCreateParams,
  TTSJobStatusResponse,
  VoiceListResponse,
} from "./types";

const BASE = "";

// Direct backend URL for large uploads (bypasses Next.js proxy 10MB limit)
const API_DIRECT =
  process.env.NEXT_PUBLIC_API_URL || "http://localhost:8000";

/**
 * Extract cover image from an EPUB file using epub.js.
 *
 * epub.js reliably locates covers across EPUB 2/3 formats via `book.coverUrl()`,
 * which is more reliable than backend ZipFile-based parsing and has no size limit.
 * The returned data URL is sent as an extra form field during upload so the
 * backend can use it directly instead of its own (size-limited) extraction.
 */
async function extractEpubCover(file: File): Promise<string | null> {
  try {
    const ePub = (await import("epubjs")).default;
    const arrayBuffer = await file.arrayBuffer();
    const book = ePub(arrayBuffer);
    await book.ready;

    const coverUrl = await book.coverUrl();
    if (!coverUrl) {
      book.destroy();
      return null;
    }

    // Convert the blob URL produced by epub.js into a base64 data URL
    // that the backend can store directly.
    const response = await fetch(coverUrl);
    const blob = await response.blob();
    const dataUrl = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onloadend = () => resolve(reader.result as string);
      reader.onerror = reject;
      reader.readAsDataURL(blob);
    });

    URL.revokeObjectURL(coverUrl);
    book.destroy();
    return dataUrl;
  } catch {
    // Cover extraction failure should never block the upload
    return null;
  }
}

interface FetchItemsParams {
  category?: string;
  search?: string;
  folder_id?: string;
  type?: string;
  page?: number;
  page_size?: number;
  sort_by?: string;
  sort_order?: string;
  progress_min?: number;
  progress_max?: number;
}

export async function fetchItems(
  params: FetchItemsParams = {}
): Promise<LibraryItemsResponse> {
  const url = new URL("/api/library/items", window.location.origin);

  Object.entries(params).forEach(([key, value]) => {
    if (value !== undefined && value !== null) {
      url.searchParams.set(key, String(value));
    }
  });

  const res = await fetch(url.toString());
  if (!res.ok) throw new Error(`Failed to fetch items: ${res.status}`);
  return res.json();
}

export async function fetchItem(id: string): Promise<LibraryItem> {
  const res = await fetch(`${BASE}/api/library/items/${id}`);
  if (!res.ok) throw new Error(`Failed to fetch item: ${res.status}`);
  return res.json();
}

export async function deleteItem(id: string): Promise<void> {
  const res = await fetch(`${BASE}/api/library/items/${id}`, {
    method: "DELETE",
  });
  if (!res.ok) throw new Error(`Failed to delete item: ${res.status}`);
}

export async function searchItems(
  query: string
): Promise<LibraryItemsResponse> {
  return fetchItems({ search: query, page_size: 50 });
}

export async function importFile(
  file: File,
  options: { folder_id?: string; category?: string; title?: string } = {}
): Promise<LibraryItem> {
  const formData = new FormData();
  formData.append("file", file);
  formData.append("folder_id", options.folder_id ?? "f1");
  formData.append("category", options.category ?? "imported");
  if (options.title) formData.append("title", options.title);

  // For EPUB files, extract the cover on the frontend using epub.js — it's more
  // reliable than backend ZipFile parsing and has no size limit.
  if (file.name.toLowerCase().endsWith(".epub")) {
    try {
      const cover = await extractEpubCover(file);
      if (cover) {
        formData.append("cover_image", cover);
      }
    } catch {
      // Never block the upload on cover extraction failure
    }
  }

  const res = await fetch(`${API_DIRECT}/api/library/import/file`, {
    method: "POST",
    body: formData,
  });
  if (!res.ok) {
    const body = await res.json().catch(() => null);
    throw new Error(body?.detail ?? `Import failed: ${res.status}`);
  }
  return res.json();
}

export async function importUrl(
  url: string,
  options: { folder_id?: string; category?: string; title?: string } = {}
): Promise<LibraryItem> {
  const res = await fetch(`${BASE}/api/library/import/url`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      url,
      folder_id: options.folder_id ?? "f1",
      category: options.category ?? "imported",
      title: options.title || undefined,
    }),
  });
  if (!res.ok) {
    const body = await res.json().catch(() => null);
    throw new Error(body?.detail ?? `Import failed: ${res.status}`);
  }
  return res.json();
}

export async function fetchItemFile(id: string): Promise<ArrayBuffer> {
  const res = await fetch(`${BASE}/api/library/items/${id}/file`);
  if (!res.ok) throw new Error(`Failed to fetch file: ${res.status}`);
  return res.arrayBuffer();
}

export async function createTTSJob(
  params: TTSJobCreateParams
): Promise<TTSJobStatusResponse> {
  const res = await fetch(`${BASE}/api/tts/jobs`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(params),
  });
  if (!res.ok) throw new Error(`Failed to create TTS job: ${res.status}`);
  return res.json();
}

export async function pollTTSJob(
  jobId: string,
  includeAudio: boolean = false
): Promise<TTSJobStatusResponse> {
  const url = new URL(`/api/tts/jobs/${jobId}`, window.location.origin);
  if (includeAudio) url.searchParams.set("include_audio", "true");
  const res = await fetch(url.toString());
  if (!res.ok) throw new Error(`Failed to poll TTS job: ${res.status}`);
  return res.json();
}

export async function cancelTTSSession(
  sessionId: string,
  keepItemId?: string,
  keepChapterId?: string
): Promise<{ cancelled_job_ids: string[] }> {
  const res = await fetch(`${BASE}/api/tts/sessions/${sessionId}/cancel`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      keep_item_id: keepItemId ?? null,
      keep_chapter_id: keepChapterId ?? null,
    }),
  });
  if (!res.ok)
    throw new Error(`Failed to cancel TTS session: ${res.status}`);
  return res.json();
}

export async function updateProgress(
  id: string,
  progress: number
): Promise<void> {
  await fetch(`${BASE}/api/library/items/${id}/progress`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ progress }),
  });
}

export async function updateVoice(
  id: string,
  voice: string | null
): Promise<void> {
  await fetch(`${BASE}/api/library/items/${id}/voice`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ voice }),
  });
}

export async function updateSpeed(
  id: string,
  speed: number | null
): Promise<void> {
  await fetch(`${BASE}/api/library/items/${id}/speed`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ speed }),
  });
}

export async function updateChapter(
  id: string,
  chapter: string | null
): Promise<void> {
  await fetch(`${BASE}/api/library/items/${id}/chapter`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ chapter }),
  });
}

export async function fetchVoices(): Promise<VoiceListResponse> {
  const res = await fetch(`${BASE}/api/tts/voices`);
  if (!res.ok) throw new Error(`Failed to fetch voices: ${res.status}`);
  return res.json();
}

// ── User settings persistence ────────────────────────────────────────────

export async function fetchSettings(): Promise<Record<string, string>> {
  const res = await fetch(`${BASE}/api/settings`);
  if (!res.ok) throw new Error(`Failed to fetch settings: ${res.status}`);
  return res.json();
}

export async function patchSettings(
  data: Record<string, string>
): Promise<void> {
  await fetch(`${BASE}/api/settings`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
}
