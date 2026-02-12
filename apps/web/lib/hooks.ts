"use client";

import useSWR from "swr";
import { useEffect, useState } from "react";
import { fetchItems, fetchItem, searchItems } from "./api";
import { LibraryItem, LibraryItemsResponse } from "./types";

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
    fetchItem(id!)
  );
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
