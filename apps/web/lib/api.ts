import {
  LibraryItem,
  LibraryItemsResponse,
  TTSJobCreateParams,
  TTSJobStatusResponse,
} from "./types";

const BASE = "";

// Direct backend URL for large uploads (bypasses Next.js proxy 10MB limit)
const API_DIRECT =
  process.env.NEXT_PUBLIC_API_URL || "http://localhost:8000";

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
