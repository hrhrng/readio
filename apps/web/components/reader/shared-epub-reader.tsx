"use client";

import { useEffect, useMemo, useRef } from "react";
import { fetchItemFile } from "@/lib/api";
import {
  createEpubAssetResolver,
  type ObjectURLStore,
} from "@/lib/epub-asset-resolver";
import { projectParsedEPUB } from "@/lib/epub-publication";
import { webReadioSdk } from "@/lib/readio-core-runtime";
import type { ParsedEPUB, ReaderIR } from "@/lib/rust-core";
import type { Sentence } from "@/lib/types";
import type { TOCItem } from "./toc-panel";
import { ReaderDocumentView } from "./reader-document-view";

interface SharedEpubReaderProps {
  itemId: string;
  content: string;
  onSentencesExtracted: (sentences: Sentence[]) => void;
  onReaderIRExtracted?: (reader: ReaderIR) => void;
  currentSentenceIndex: number;
  currentWordProgress: number;
  onSentenceClick: (index: number) => void;
  onChaptersExtracted?: (chapters: TOCItem[]) => void;
  assetIO?: SharedEpubAssetIO;
}

export interface SharedEpubAssetIO {
  loadArchive(itemId: string): Promise<Uint8Array>;
  extractAsset(archive: Uint8Array, assetId: string): Promise<Uint8Array>;
  objectURLs?: ObjectURLStore;
}

const defaultAssetIO: SharedEpubAssetIO = {
  async loadArchive(itemId) {
    return new Uint8Array(await fetchItemFile(itemId));
  },
  extractAsset(archive, assetId) {
    return webReadioSdk.extractEPUBAsset(archive, assetId);
  },
};

export function SharedEpubReader({
  itemId,
  content,
  onSentencesExtracted,
  onReaderIRExtracted,
  currentSentenceIndex,
  currentWordProgress,
  onSentenceClick,
  onChaptersExtracted,
  assetIO = defaultAssetIO,
}: SharedEpubReaderProps) {
  const emittedReaderRef = useRef<ReaderIR | null>(null);
  const result = useMemo(() => {
    try {
      const parsed = JSON.parse(content) as ParsedEPUB;
      return {
        publication: projectParsedEPUB(parsed),
        assets: parsed.assets ?? [],
        error: null,
      };
    } catch (error) {
      return {
        publication: null,
        assets: [],
        error:
          error instanceof Error
            ? error.message
            : "Invalid Readio EPUB publication",
      };
    }
  }, [content]);
  const assetResolver = useMemo(() => {
    if (result.assets.length === 0) return null;
    return createEpubAssetResolver({
      assets: result.assets,
      loadArchive: () => assetIO.loadArchive(itemId),
      extractAsset: assetIO.extractAsset,
      objectURLs: assetIO.objectURLs,
    });
  }, [assetIO, itemId, result.assets]);
  const resolveImageAsset = useMemo(
    () =>
      assetResolver
        ? (assetId: string) => assetResolver.resolve(assetId)
        : undefined,
    [assetResolver]
  );

  useEffect(
    () => () => {
      assetResolver?.dispose();
    },
    [assetResolver]
  );

  useEffect(() => {
    if (
      !result.publication
      || emittedReaderRef.current === result.publication.reader
    ) return;
    emittedReaderRef.current = result.publication.reader;
    onSentencesExtracted(result.publication.sentences);
    onReaderIRExtracted?.(result.publication.reader);
    onChaptersExtracted?.(result.publication.toc);
  }, [
    onChaptersExtracted,
    onReaderIRExtracted,
    onSentencesExtracted,
    result.publication,
  ]);

  useEffect(() => {
    if (currentSentenceIndex < 0) return;
    document
      .querySelector(`[data-sentence-id="${currentSentenceIndex}"]`)
      ?.scrollIntoView({ behavior: "smooth", block: "center" });
  }, [currentSentenceIndex]);

  if (!result.publication) {
    return (
      <div className="text-center py-12 text-text-secondary" role="alert">
        <p>Failed to load EPUB: {result.error}</p>
      </div>
    );
  }

  return (
    <ReaderDocumentView
      projection={result.publication}
      currentSentenceIndex={currentSentenceIndex}
      currentWordProgress={currentWordProgress}
      onSentenceClick={onSentenceClick}
      resolveImageAsset={resolveImageAsset}
    />
  );
}
