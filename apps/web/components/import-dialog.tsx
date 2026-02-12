"use client";

import { useState, useRef, useCallback } from "react";
import { X, Upload, Link, FileText, Loader2 } from "lucide-react";
import { importFile, importUrl } from "@/lib/api";
import { useSWRConfig } from "swr";

interface ImportDialogProps {
  open: boolean;
  onClose: () => void;
}

const ACCEPTED_EXTENSIONS = ".txt,.pdf,.epub,.docx";
const ACCEPTED_TYPES = [
  "text/plain",
  "application/pdf",
  "application/epub+zip",
  "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
];

export function ImportDialog({ open, onClose }: ImportDialogProps) {
  const { mutate } = useSWRConfig();
  const [tab, setTab] = useState<"file" | "url">("file");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dragActive, setDragActive] = useState(false);
  const [url, setUrl] = useState("");
  const fileInputRef = useRef<HTMLInputElement>(null);
  const overlayRef = useRef<HTMLDivElement>(null);

  const revalidate = useCallback(() => {
    // Revalidate all library queries without clearing cached data
    mutate(
      (key: unknown) =>
        Array.isArray(key) &&
        (key[0] === "library-items" || key[0] === "search")
    );
  }, [mutate]);

  const handleFileImport = useCallback(
    async (file: File) => {
      setLoading(true);
      setError(null);
      try {
        await importFile(file);
        revalidate();
        onClose();
      } catch (err) {
        setError(err instanceof Error ? err.message : "Import failed");
      } finally {
        setLoading(false);
      }
    },
    [revalidate, onClose]
  );

  const handleUrlImport = useCallback(async () => {
    const trimmed = url.trim();
    if (!trimmed) return;
    setLoading(true);
    setError(null);
    try {
      await importUrl(trimmed);
      revalidate();
      setUrl("");
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Import failed");
    } finally {
      setLoading(false);
    }
  }, [url, revalidate, onClose]);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setDragActive(false);
      const file = e.dataTransfer.files[0];
      if (file) handleFileImport(file);
    },
    [handleFileImport]
  );

  const handleFileChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (file) handleFileImport(file);
      // Reset so the same file can be selected again
      e.target.value = "";
    },
    [handleFileImport]
  );

  if (!open) return null;

  return (
    <div
      ref={overlayRef}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      onClick={(e) => {
        if (e.target === overlayRef.current && !loading) onClose();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape" && !loading) onClose();
      }}
    >
      <div className="bg-surface-card rounded-2xl shadow-xl max-w-md w-full mx-4 overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between px-6 pt-5 pb-4">
          <h2 className="text-lg font-semibold text-text-primary">
            Import Content
          </h2>
          <button
            onClick={onClose}
            disabled={loading}
            className="w-8 h-8 flex items-center justify-center rounded-lg hover:bg-surface-hover transition-colors text-text-secondary cursor-pointer"
            aria-label="Close"
          >
            <X size={18} />
          </button>
        </div>

        {/* Tabs */}
        <div className="flex gap-1 px-6 mb-4">
          <button
            onClick={() => {
              setTab("file");
              setError(null);
            }}
            className={`flex items-center gap-2 px-4 py-2 text-sm font-medium rounded-lg transition-colors cursor-pointer ${
              tab === "file"
                ? "bg-accent/10 text-accent"
                : "text-text-secondary hover:bg-surface-hover"
            }`}
          >
            <Upload size={16} />
            File
          </button>
          <button
            onClick={() => {
              setTab("url");
              setError(null);
            }}
            className={`flex items-center gap-2 px-4 py-2 text-sm font-medium rounded-lg transition-colors cursor-pointer ${
              tab === "url"
                ? "bg-accent/10 text-accent"
                : "text-text-secondary hover:bg-surface-hover"
            }`}
          >
            <Link size={16} />
            URL
          </button>
        </div>

        {/* Content */}
        <div className="px-6 pb-6">
          {tab === "file" ? (
            <>
              <div
                className={`relative border-2 border-dashed rounded-xl p-8 text-center transition-colors ${
                  dragActive
                    ? "border-accent bg-accent/5"
                    : "border-border hover:border-text-tertiary"
                } ${loading ? "pointer-events-none opacity-50" : "cursor-pointer"}`}
                onDragOver={(e) => {
                  e.preventDefault();
                  setDragActive(true);
                }}
                onDragLeave={() => setDragActive(false)}
                onDrop={handleDrop}
                onClick={() => fileInputRef.current?.click()}
              >
                {loading ? (
                  <Loader2
                    size={32}
                    className="mx-auto text-accent animate-spin"
                  />
                ) : (
                  <FileText
                    size={32}
                    className="mx-auto text-text-tertiary mb-3"
                  />
                )}
                <p className="text-sm text-text-primary font-medium mt-2">
                  {loading
                    ? "Importing..."
                    : "Drop file here or click to browse"}
                </p>
                <p className="text-xs text-text-tertiary mt-2">
                  TXT, PDF, EPUB, DOCX
                </p>
                <input
                  ref={fileInputRef}
                  type="file"
                  className="hidden"
                  accept={ACCEPTED_EXTENSIONS}
                  onChange={handleFileChange}
                />
              </div>
            </>
          ) : (
            <div className="space-y-3">
              <input
                type="url"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://example.com/article"
                disabled={loading}
                className="w-full px-4 py-3 rounded-xl bg-surface border border-border text-sm text-text-primary placeholder:text-text-tertiary focus:outline-none focus:ring-2 focus:ring-accent/50 disabled:opacity-50"
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleUrlImport();
                }}
              />
              <button
                onClick={handleUrlImport}
                disabled={loading || !url.trim()}
                className="w-full py-3 rounded-xl bg-accent text-white text-sm font-medium hover:bg-accent/90 transition-colors disabled:opacity-50 cursor-pointer flex items-center justify-center gap-2"
              >
                {loading ? (
                  <>
                    <Loader2 size={16} className="animate-spin" />
                    Importing...
                  </>
                ) : (
                  "Import URL"
                )}
              </button>
            </div>
          )}

          {/* Error */}
          {error && (
            <p className="mt-3 text-sm text-red-500 text-center">{error}</p>
          )}
        </div>
      </div>
    </div>
  );
}
