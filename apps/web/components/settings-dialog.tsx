"use client";

import { useTheme } from "next-themes";
import { useEffect, useRef, useState, useCallback } from "react";
import { createPortal } from "react-dom";
import { Sun, Moon, Monitor, Check, X, Eye, EyeOff, Trash2, ChevronDown, Search } from "lucide-react";
import { useSettings } from "@/lib/hooks";
import { applyAccentColor, applyFontSize } from "@/lib/settings-utils";
import { fetchApiKeys, fetchVoicesByProvider, fetchModels, patchSettings, type ApiKeyStatus, type ModelInfo } from "@/lib/api";
import type { VoiceInfo } from "@/lib/types";

interface SettingsDialogProps {
  open: boolean;
  onClose: () => void;
}

const NAV_ITEMS = [
  { id: "appearance", label: "Appearance" },
  { id: "reading", label: "Reading" },
  { id: "api-keys", label: "API Keys" },
  { id: "about", label: "About" },
] as const;

type SectionId = (typeof NAV_ITEMS)[number]["id"];

export const ACCENT_PRESETS = [
  { name: "Sage", light: "#6B8F71", dark: "#7EC8A0" },
  { name: "Blue", light: "#4A7FBF", dark: "#6EB5FF" },
  { name: "Amber", light: "#B8860B", dark: "#E8B84B" },
  { name: "Rose", light: "#C26B7E", dark: "#E8909F" },
  { name: "Violet", light: "#7C6BAF", dark: "#A594D8" },
] as const;

/**
 * Full-screen modal overlay for settings.
 * Follows the same left-nav + right-content layout as Claude's settings dialog.
 * All settings are persisted to the backend via the useSettings hook.
 */
export function SettingsDialog({ open, onClose }: SettingsDialogProps) {
  const overlayRef = useRef<HTMLDivElement>(null);
  const { theme, setTheme } = useTheme();
  const [mounted, setMounted] = useState(false);
  const [activeSection, setActiveSection] = useState<SectionId>("appearance");
  const { settings, updateSetting } = useSettings();

  useEffect(() => setMounted(true), []);

  // Close on ESC
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open, onClose]);

  if (!open || !mounted) return null;

  // Derive current values from persisted settings (with sensible defaults)
  const accentIndex = parseInt(settings.accent_color ?? "0", 10);
  const fontSize = parseInt(settings.font_size ?? "18", 10);
  const ttsSpeed = parseFloat(settings.tts_speed ?? "1.0");

  return createPortal(
    <div
      ref={overlayRef}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      onClick={(e) => {
        if (e.target === overlayRef.current) onClose();
      }}
    >
      {/* Dialog card */}
      <div className="bg-surface rounded-2xl shadow-2xl w-[780px] h-[560px] overflow-hidden flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-border shrink-0">
          <h2 className="text-lg font-semibold text-text-primary">Settings</h2>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg text-text-tertiary hover:text-text-primary hover:bg-surface-hover transition-colors cursor-pointer"
          >
            <X size={18} />
          </button>
        </div>

        {/* Body: left nav + right content */}
        <div className="flex flex-1 min-h-0">
          {/* Left nav */}
          <nav className="w-40 shrink-0 border-r border-border py-3 px-2">
            <ul className="flex flex-col gap-0.5">
              {NAV_ITEMS.map(({ id, label }) => (
                <li key={id}>
                  <button
                    onClick={() => setActiveSection(id)}
                    className={`w-full text-left px-3 py-1.5 rounded-lg text-sm transition-colors cursor-pointer ${
                      activeSection === id
                        ? "bg-surface-active text-text-primary font-medium"
                        : "text-text-secondary hover:bg-surface-hover hover:text-text-primary"
                    }`}
                  >
                    {label}
                  </button>
                </li>
              ))}
            </ul>
          </nav>

          {/* Right content — scrollable */}
          <div className="flex-1 overflow-y-auto p-6">
            {activeSection === "appearance" && (
              <AppearanceSection
                theme={theme}
                setTheme={setTheme}
                accentIndex={accentIndex}
                onAccentChange={(i) => {
                  updateSetting("accent_color", String(i));
                  applyAccentColor(i);
                }}
              />
            )}
            {activeSection === "reading" && (
              <ReadingSection
                fontSize={fontSize}
                ttsSpeed={ttsSpeed}
                onFontSizeChange={(size) => {
                  updateSetting("font_size", String(size));
                  applyFontSize(size);
                }}
                onTtsSpeedChange={(speed) => {
                  updateSetting("tts_speed", speed.toFixed(1));
                }}
              />
            )}
            {activeSection === "api-keys" && <ApiKeysSection />}
            {activeSection === "about" && <AboutSection />}
          </div>
        </div>
      </div>
    </div>,
    document.body
  );
}

/* ─────────────── Section: Appearance ─────────────── */

function AppearanceSection({
  theme,
  setTheme,
  accentIndex,
  onAccentChange,
}: {
  theme: string | undefined;
  setTheme: (t: string) => void;
  accentIndex: number;
  onAccentChange: (i: number) => void;
}) {
  const modes = [
    { value: "light", icon: Sun, label: "Light" },
    { value: "dark", icon: Moon, label: "Dark" },
    { value: "system", icon: Monitor, label: "System" },
  ] as const;

  return (
    <div className="space-y-8">
      <div>
        <SectionHeading>Color mode</SectionHeading>
        <div className="flex gap-3">
          {modes.map(({ value, icon: Icon, label }) => (
            <button
              key={value}
              onClick={() => setTheme(value)}
              className={`flex flex-col items-center gap-2 rounded-xl border-2 px-6 py-4 transition-all cursor-pointer ${
                theme === value
                  ? "border-accent bg-surface-active"
                  : "border-border hover:border-text-tertiary bg-surface-card"
              }`}
            >
              <Icon
                size={22}
                strokeWidth={1.5}
                className={theme === value ? "text-accent" : "text-text-secondary"}
              />
              <span
                className={`text-xs font-medium ${theme === value ? "text-accent" : "text-text-secondary"}`}
              >
                {label}
              </span>
            </button>
          ))}
        </div>
      </div>

      <Divider />

      <div>
        <SectionHeading>Accent color</SectionHeading>
        <div className="flex gap-3">
          {ACCENT_PRESETS.map((preset, i) => (
            <button
              key={preset.name}
              onClick={() => onAccentChange(i)}
              title={preset.name}
              className="relative w-9 h-9 rounded-full border-2 transition-all cursor-pointer flex items-center justify-center"
              style={{
                backgroundColor: preset.light,
                borderColor: accentIndex === i ? preset.light : "transparent",
                boxShadow:
                  accentIndex === i
                    ? `0 0 0 2px var(--surface-card), 0 0 0 4px ${preset.light}`
                    : "none",
              }}
            >
              {accentIndex === i && (
                <Check size={16} strokeWidth={2.5} className="text-white" />
              )}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

/* ─────────────── Section: Reading ─────────────── */

function ReadingSection({
  fontSize,
  ttsSpeed,
  onFontSizeChange,
  onTtsSpeedChange,
}: {
  fontSize: number;
  ttsSpeed: number;
  onFontSizeChange: (size: number) => void;
  onTtsSpeedChange: (speed: number) => void;
}) {
  return (
    <div className="space-y-8">
      <div>
        <SectionHeading>Default font size</SectionHeading>
        <p className="text-sm text-text-tertiary mb-3">
          Adjust the default reading font size across all books.
        </p>
        <div className="flex items-center gap-4">
          <input
            type="range"
            min={14}
            max={24}
            value={fontSize}
            onChange={(e) => onFontSizeChange(Number(e.target.value))}
            className="flex-1 accent-accent"
          />
          <span className="text-sm text-text-secondary font-medium tabular-nums w-10 text-right">
            {fontSize}px
          </span>
        </div>
      </div>

      <Divider />

      <div>
        <SectionHeading>TTS speed</SectionHeading>
        <p className="text-sm text-text-tertiary mb-3">
          Default playback speed for text-to-speech.
        </p>
        <div className="flex items-center gap-4">
          <input
            type="range"
            min={0.5}
            max={2}
            step={0.1}
            value={ttsSpeed}
            onChange={(e) => onTtsSpeedChange(Number(e.target.value))}
            className="flex-1 accent-accent"
          />
          <span className="text-sm text-text-secondary font-medium tabular-nums w-10 text-right">
            {ttsSpeed.toFixed(1)}x
          </span>
        </div>
      </div>
    </div>
  );
}

/* ─────────────── Section: API Keys ─────────────── */

interface ApiKeyFieldConfig {
  key: string;
  label: string;
  placeholder: string;
  isSecret: boolean;
  fieldType: "text" | "voice" | "model";
  provider: string;
}

const ELEVENLABS_FIELDS: ApiKeyFieldConfig[] = [
  { key: "elevenlabs_api_key", label: "API Key", placeholder: "sk-...", isSecret: true, fieldType: "text", provider: "elevenlabs" },
  { key: "elevenlabs_model_id", label: "Model ID", placeholder: "eleven_multilingual_v2", isSecret: false, fieldType: "model", provider: "elevenlabs" },
  { key: "elevenlabs_voice_id", label: "Voice ID", placeholder: "Default voice ID", isSecret: false, fieldType: "voice", provider: "elevenlabs" },
];

const MINIMAX_FIELDS: ApiKeyFieldConfig[] = [
  { key: "minimax_api_key", label: "API Key", placeholder: "eyJ...", isSecret: true, fieldType: "text", provider: "minimax" },
  { key: "minimax_group_id", label: "Group ID", placeholder: "Optional", isSecret: false, fieldType: "text", provider: "minimax" },
  { key: "minimax_model_id", label: "Model ID", placeholder: "speech-2.6-hd", isSecret: false, fieldType: "model", provider: "minimax" },
  { key: "minimax_voice_id", label: "Voice ID", placeholder: "Default voice ID", isSecret: false, fieldType: "voice", provider: "minimax" },
];

function ApiKeysSection() {
  const [keyStatus, setKeyStatus] = useState<Record<string, ApiKeyStatus>>({});
  const [loading, setLoading] = useState(true);

  const loadKeys = useCallback(async () => {
    try {
      const data = await fetchApiKeys();
      setKeyStatus(data);
    } catch {
      // ignore
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadKeys();
  }, [loadKeys]);

  if (loading) {
    return (
      <div className="space-y-4">
        <SectionHeading>API Keys</SectionHeading>
        <p className="text-sm text-text-tertiary">Loading...</p>
      </div>
    );
  }

  return (
    <div className="space-y-8">
      <div>
        <SectionHeading>API Keys</SectionHeading>
        <p className="text-sm text-text-tertiary mb-4">
          Configure your own TTS provider API keys. When not set, the global server keys will be used as fallback.
        </p>
      </div>

      <ProviderSection
        title="ElevenLabs"
        fields={ELEVENLABS_FIELDS}
        keyStatus={keyStatus}
        onUpdate={loadKeys}
      />

      <Divider />

      <ProviderSection
        title="MiniMax"
        fields={MINIMAX_FIELDS}
        keyStatus={keyStatus}
        onUpdate={loadKeys}
      />
    </div>
  );
}

function ProviderSection({
  title,
  fields,
  keyStatus,
  onUpdate,
}: {
  title: string;
  fields: ApiKeyFieldConfig[];
  keyStatus: Record<string, ApiKeyStatus>;
  onUpdate: () => void;
}) {
  return (
    <div>
      <h4 className="text-sm font-medium text-text-primary mb-3">{title}</h4>
      <div className="space-y-3">
        {fields.map((field) => {
          if (field.fieldType === "voice" || field.fieldType === "model") {
            return (
              <SelectableField
                key={field.key}
                config={field}
                status={keyStatus[field.key]}
                keyStatus={keyStatus}
                onUpdate={onUpdate}
              />
            );
          }
          return (
            <ApiKeyField
              key={field.key}
              config={field}
              status={keyStatus[field.key]}
              onUpdate={onUpdate}
            />
          );
        })}
      </div>
    </div>
  );
}

function ApiKeyField({
  config,
  status,
  onUpdate,
}: {
  config: ApiKeyFieldConfig;
  status?: ApiKeyStatus;
  onUpdate: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState("");
  const [saving, setSaving] = useState(false);
  const [showValue, setShowValue] = useState(false);

  const isConfigured = status?.configured ?? false;
  const hasGlobalFallback = status?.has_global_fallback ?? false;

  async function handleSave() {
    if (!value.trim()) return;
    setSaving(true);
    try {
      await patchSettings({ [config.key]: value.trim() });
      setValue("");
      setEditing(false);
      onUpdate();
    } finally {
      setSaving(false);
    }
  }

  async function handleRemove() {
    setSaving(true);
    try {
      await patchSettings({ [config.key]: null as unknown as string });
      onUpdate();
    } finally {
      setSaving(false);
    }
  }

  if (editing) {
    return (
      <div className="flex items-center gap-2">
        <label className="w-20 text-xs text-text-secondary shrink-0">{config.label}</label>
        <input
          type={config.isSecret && !showValue ? "password" : "text"}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          placeholder={config.placeholder}
          className="flex-1 rounded-lg border border-border bg-surface-card px-3 py-1.5 text-sm text-text-primary placeholder:text-text-tertiary focus:outline-none focus:ring-2 focus:ring-accent"
          autoFocus
          onKeyDown={(e) => {
            if (e.key === "Enter") handleSave();
            if (e.key === "Escape") { setEditing(false); setValue(""); }
          }}
        />
        {config.isSecret && (
          <button
            type="button"
            onClick={() => setShowValue(!showValue)}
            className="p-1.5 text-text-tertiary hover:text-text-secondary cursor-pointer"
          >
            {showValue ? <EyeOff size={14} /> : <Eye size={14} />}
          </button>
        )}
        <button
          onClick={handleSave}
          disabled={saving || !value.trim()}
          className="px-3 py-1.5 text-xs font-medium rounded-lg bg-accent text-white hover:opacity-90 disabled:opacity-50 cursor-pointer"
        >
          Save
        </button>
        <button
          onClick={() => { setEditing(false); setValue(""); }}
          className="px-2 py-1.5 text-xs text-text-tertiary hover:text-text-primary cursor-pointer"
        >
          Cancel
        </button>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-2">
      <label className="w-20 text-xs text-text-secondary shrink-0">{config.label}</label>
      <div className="flex-1 text-sm">
        {isConfigured ? (
          <span className="text-text-primary font-mono text-xs">
            {status?.masked ?? "Configured"}
          </span>
        ) : (
          <span className="text-text-tertiary text-xs">
            Not set
            {hasGlobalFallback && " (using server default)"}
          </span>
        )}
      </div>
      <button
        onClick={() => setEditing(true)}
        className="px-3 py-1.5 text-xs font-medium rounded-lg border border-border text-text-secondary hover:text-text-primary hover:bg-surface-hover cursor-pointer"
      >
        {isConfigured ? "Change" : "Set"}
      </button>
      {isConfigured && (
        <button
          onClick={handleRemove}
          disabled={saving}
          className="p-1.5 text-text-tertiary hover:text-red-500 cursor-pointer disabled:opacity-50"
          title="Remove"
        >
          <Trash2 size={14} />
        </button>
      )}
    </div>
  );
}

/* ─────────────── Selectable Field (Voice/Model picker) ─────────────── */

function SelectableField({
  config,
  status,
  keyStatus,
  onUpdate,
}: {
  config: ApiKeyFieldConfig;
  status?: ApiKeyStatus;
  keyStatus: Record<string, ApiKeyStatus>;
  onUpdate: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [manualMode, setManualMode] = useState(false);
  const [manualValue, setManualValue] = useState("");
  const [saving, setSaving] = useState(false);
  const [search, setSearch] = useState("");
  const [items, setItems] = useState<Array<{ id: string; label: string; description: string | null }>>([]);
  const [defaultId, setDefaultId] = useState("");
  const [loadingItems, setLoadingItems] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  const isConfigured = status?.configured ?? false;
  const currentValue = status?.masked ?? null;

  // Check if provider API key is available (for ElevenLabs voice browsing)
  const apiKeyField = `${config.provider}_api_key`;
  const hasApiKey = keyStatus[apiKeyField]?.configured || keyStatus[apiKeyField]?.has_global_fallback;

  // Close dropdown on outside click
  useEffect(() => {
    if (!expanded) return;
    const handler = (e: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setExpanded(false);
        setSearch("");
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [expanded]);

  // Focus search when expanded
  useEffect(() => {
    if (expanded && searchRef.current) {
      searchRef.current.focus();
    }
  }, [expanded]);

  async function loadItems() {
    setLoadingItems(true);
    setError(null);
    try {
      if (config.fieldType === "voice") {
        const data = await fetchVoicesByProvider(config.provider);
        setItems(data.voices.map((v: VoiceInfo) => ({ id: v.voice_id, label: v.label, description: v.description })));
        setDefaultId(data.default_voice_id);
      } else {
        const data = await fetchModels(config.provider);
        setItems(data.models.map((m: ModelInfo) => ({ id: m.model_id, label: m.label, description: m.description })));
        setDefaultId(data.default_model_id);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load");
    } finally {
      setLoadingItems(false);
    }
  }

  function handleExpand() {
    if (config.fieldType === "voice" && config.provider === "elevenlabs" && !hasApiKey) {
      setError("Set API key first to browse voices");
      setExpanded(true);
      return;
    }
    setExpanded(true);
    loadItems();
  }

  async function handleSelect(id: string) {
    setSaving(true);
    try {
      await patchSettings({ [config.key]: id });
      setExpanded(false);
      setSearch("");
      onUpdate();
    } finally {
      setSaving(false);
    }
  }

  async function handleManualSave() {
    if (!manualValue.trim()) return;
    setSaving(true);
    try {
      await patchSettings({ [config.key]: manualValue.trim() });
      setManualValue("");
      setManualMode(false);
      onUpdate();
    } finally {
      setSaving(false);
    }
  }

  async function handleRemove() {
    setSaving(true);
    try {
      await patchSettings({ [config.key]: null as unknown as string });
      onUpdate();
    } finally {
      setSaving(false);
    }
  }

  if (manualMode) {
    return (
      <div className="flex items-center gap-2">
        <label className="w-20 text-xs text-text-secondary shrink-0">{config.label}</label>
        <input
          type="text"
          value={manualValue}
          onChange={(e) => setManualValue(e.target.value)}
          placeholder={config.placeholder}
          className="flex-1 rounded-lg border border-border bg-surface-card px-3 py-1.5 text-sm text-text-primary placeholder:text-text-tertiary focus:outline-none focus:ring-2 focus:ring-accent"
          autoFocus
          onKeyDown={(e) => {
            if (e.key === "Enter") handleManualSave();
            if (e.key === "Escape") { setManualMode(false); setManualValue(""); }
          }}
        />
        <button
          onClick={handleManualSave}
          disabled={saving || !manualValue.trim()}
          className="px-3 py-1.5 text-xs font-medium rounded-lg bg-accent text-white hover:opacity-90 disabled:opacity-50 cursor-pointer"
        >
          Save
        </button>
        <button
          onClick={() => { setManualMode(false); setManualValue(""); }}
          className="px-2 py-1.5 text-xs text-text-tertiary hover:text-text-primary cursor-pointer"
        >
          Cancel
        </button>
      </div>
    );
  }

  const filtered = items.filter(
    (item) =>
      item.label.toLowerCase().includes(search.toLowerCase()) ||
      item.id.toLowerCase().includes(search.toLowerCase())
  );

  return (
    <div className="relative" ref={dropdownRef}>
      <div className="flex items-center gap-2">
        <label className="w-20 text-xs text-text-secondary shrink-0">{config.label}</label>
        <div className="flex-1 text-sm">
          {isConfigured ? (
            <span className="text-text-primary font-mono text-xs">{currentValue}</span>
          ) : (
            <span className="text-text-tertiary text-xs">
              Not set
              {status?.has_global_fallback && " (using server default)"}
            </span>
          )}
        </div>
        <button
          onClick={handleExpand}
          className="px-3 py-1.5 text-xs font-medium rounded-lg border border-border text-text-secondary hover:text-text-primary hover:bg-surface-hover cursor-pointer inline-flex items-center gap-1"
        >
          <ChevronDown size={12} />
          {isConfigured ? "Change" : "Select"}
        </button>
        {isConfigured && (
          <button
            onClick={handleRemove}
            disabled={saving}
            className="p-1.5 text-text-tertiary hover:text-red-500 cursor-pointer disabled:opacity-50"
            title="Remove"
          >
            <Trash2 size={14} />
          </button>
        )}
      </div>

      {expanded && (
        <div className="absolute left-20 right-0 top-full mt-1 z-10 bg-surface-card border border-border rounded-xl shadow-lg overflow-hidden">
          {/* Search */}
          <div className="flex items-center gap-2 px-3 py-2 border-b border-border">
            <Search size={14} className="text-text-tertiary shrink-0" />
            <input
              ref={searchRef}
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search..."
              className="flex-1 text-sm bg-transparent text-text-primary placeholder:text-text-tertiary outline-none"
            />
          </div>

          {/* List */}
          <div className="max-h-52 overflow-y-auto">
            {loadingItems && (
              <div className="px-3 py-4 text-xs text-text-tertiary text-center">Loading...</div>
            )}
            {error && (
              <div className="px-3 py-4 text-xs text-text-tertiary text-center">{error}</div>
            )}
            {!loadingItems && !error && filtered.length === 0 && (
              <div className="px-3 py-4 text-xs text-text-tertiary text-center">No results</div>
            )}
            {!loadingItems && !error && filtered.map((item) => (
              <button
                key={item.id}
                onClick={() => handleSelect(item.id)}
                disabled={saving}
                className="w-full text-left px-3 py-2 hover:bg-surface-hover transition-colors cursor-pointer flex flex-col gap-0.5"
              >
                <span className="text-sm text-text-primary flex items-center gap-2">
                  {item.label}
                  {item.id === defaultId && (
                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-accent/10 text-accent font-medium">default</span>
                  )}
                  {item.id === currentValue && (
                    <Check size={14} className="text-accent" />
                  )}
                </span>
                <span className="text-xs text-text-tertiary font-mono">{item.id}</span>
                {item.description && (
                  <span className="text-xs text-text-tertiary">{item.description}</span>
                )}
              </button>
            ))}
          </div>

          {/* Footer: type manually */}
          <div className="border-t border-border px-3 py-2">
            <button
              onClick={() => { setExpanded(false); setSearch(""); setManualMode(true); }}
              className="text-xs text-accent hover:underline cursor-pointer"
            >
              Type manually
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

/* ─────────────── Section: About ─────────────── */

function AboutSection() {
  return (
    <div className="space-y-4">
      <SectionHeading>About</SectionHeading>
      <div className="bg-surface-card border border-border rounded-xl p-5 space-y-3">
        <div className="flex items-baseline gap-3">
          <span className="text-lg font-serif font-bold text-text-primary">
            Readio
          </span>
          <span className="text-xs text-text-tertiary">v0.1.0</span>
        </div>
        <p className="text-sm text-text-secondary leading-relaxed">
          Open-source audiobook &amp; TTS reading platform. Turn any text, URL,
          EPUB, or PDF into a listenable experience.
        </p>
        <a
          href="https://github.com/nicepkg/readio"
          target="_blank"
          rel="noopener noreferrer"
          className="inline-block text-sm text-accent hover:underline"
        >
          View on GitHub
        </a>
      </div>
    </div>
  );
}

/* ─── Shared helpers ─── */

function SectionHeading({ children }: { children: React.ReactNode }) {
  return <h3 className="text-sm font-medium text-text-primary mb-2">{children}</h3>;
}

function Divider() {
  return <div className="border-t border-border" />;
}
