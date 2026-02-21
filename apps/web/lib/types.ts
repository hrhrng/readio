export type LibraryItemType = "web" | "txt" | "pdf" | "epub";
export type LibraryItemCategory = "imported" | "podcasts";

export interface LibraryItem {
  id: string;
  title: string;
  content: string;
  type: LibraryItemType;
  progress: number;
  date: string;
  category: LibraryItemCategory;
  folder_id: string;
  source: string;
  file_path: string | null;
  cover_image: string | null;
  voice: string | null;
}

export interface LibraryItemsResponse {
  items: LibraryItem[];
  total: number;
  page: number;
  page_size: number;
  total_pages: number;
}

export type ReadingStatus = "new" | "reading" | "finished";

export function getReadingStatus(progress: number): ReadingStatus {
  if (progress === 0) return "new";
  if (progress >= 100) return "finished";
  return "reading";
}

export interface Sentence {
  index: number;
  text: string;
}

export interface Word {
  index: number;
  text: string;
  sentenceIndex: number;
}

export interface TTSJobCreateParams {
  session_id: string;
  item_id: string;
  chapter_id: string;
  priority: "user" | "prefetch";
  provider?: string;
  request: {
    text: string;
    speed?: number;
    voice?: string;
  };
}

export interface VoiceInfo {
  voice_id: string;
  label: string;
  language: string;
  gender: string | null;
  description: string | null;
}

export interface VoiceListResponse {
  voices: VoiceInfo[];
  default_voice_id: string;
}

export interface TTSJobStatusResponse {
  job_id: string;
  session_id: string;
  item_id: string;
  chapter_id: string;
  priority: "user" | "prefetch";
  status: "queued" | "running" | "completed" | "failed" | "cancelled";
  provider?: string;
  trace_id?: string;
  duration_ms?: number;
  sample_rate?: number;
  error?: string;
  cache_hit: boolean;
  created_at_ms: number;
  started_at_ms?: number;
  finished_at_ms?: number;
  audio_base64?: string;
}
