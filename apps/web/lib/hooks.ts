"use client";

import useSWR, { mutate as globalMutate } from "swr";
import { useCallback, useEffect, useState } from "react";
import {
  fetchItems,
  fetchItem,
  searchItems,
  fetchVoices,
  fetchSettings,
  patchSettings,
} from "./api";
import { LibraryItem, LibraryItemsResponse, VoiceListResponse } from "./types";

interface UseLibraryItemsParams {
  category?: string;
  type?: string;
  page?: number;
  page_size?: number;
  sort_by?: string;
  sort_order?: string;
  progress_min?: number;
  progress_max?: number;
}

export function useLibraryItems(params: UseLibraryItemsParams = {}) {
  const key = ["library-items", JSON.stringify(params)];
  return useSWR<LibraryItemsResponse>(key, () => fetchItems(params), {
    revalidateOnFocus: false,
  });
}

export function useLibraryItem(id: string | null) {
  return useSWR<LibraryItem>(id ? ["library-item", id] : null, () =>
    fetchItem(id!),
    { revalidateOnFocus: false }
  );
}

export function useVoices() {
  return useSWR<VoiceListResponse>("tts-voices", fetchVoices, {
    revalidateOnFocus: false,
  });
}

export function useSearchItems(query: string) {
  const [debouncedQuery, setDebouncedQuery] = useState(query);

  useEffect(() => {
    const timer = setTimeout(() => setDebouncedQuery(query), 300);
    return () => clearTimeout(timer);
  }, [query]);

  return useSWR<LibraryItemsResponse>(
    debouncedQuery ? ["search", debouncedQuery] : null,
    () => searchItems(debouncedQuery),
    { revalidateOnFocus: false }
  );
}

// ── User settings (backend-persisted) ────────────────────────────────────

const SETTINGS_KEY = "user-settings";

export function useSettings() {
  const { data: settings, isLoading } = useSWR<Record<string, string>>(
    SETTINGS_KEY,
    fetchSettings,
    { revalidateOnFocus: false }
  );

  /**
   * Optimistically update a single setting key/value, then persist to backend.
   * Dispatches a `readio-settings-change` event so other components (e.g.
   * reader content area) can react immediately without polling.
   */
  const updateSetting = useCallback(
    (key: string, value: string) => {
      // Optimistic SWR mutation — merges the new key into the cached dict
      globalMutate(
        SETTINGS_KEY,
        (prev: Record<string, string> | undefined) => ({
          ...prev,
          [key]: value,
        }),
        false // don't revalidate immediately
      );

      // Fire-and-forget backend persistence
      patchSettings({ [key]: value }).catch(() => {});

      // Broadcast change so non-SWR consumers (e.g. CSS var appliers) can react
      window.dispatchEvent(
        new CustomEvent("readio-settings-change", { detail: { key, value } })
      );
    },
    []
  );

  return { settings: settings ?? {}, isLoading, updateSetting };
}
