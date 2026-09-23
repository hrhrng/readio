"use client";

import { useTheme } from "next-themes";
import { useEffect, useRef, useState, useCallback } from "react";
import { createPortal } from "react-dom";
import { Sun, Moon, Monitor, Check, X, Eye, EyeOff, Trash2, ChevronDown, Search, Download, ShieldCheck } from "lucide-react";
import { useSettings } from "@/lib/hooks";
import { applyAccentColor, applyFontSize } from "@/lib/settings-utils";
import { resolveStoredReadingPace } from "@/lib/settings-utils";
import { fetchApiKeys, fetchVoicesByProvider, fetchModels, patchSettings, type ApiKeyStatus, type ModelInfo } from "@/lib/api";
import type { VoiceInfo } from "@/lib/types";
import { ACCENT_PRESETS } from "@/lib/design-tokens";
import { BrandMark } from "./brand-mark";
import { useToast } from "./toast-provider";
import { useLocale } from "./locale-provider";
import { supportBundleFile, webDiagnostics } from "@/lib/diagnostics";
import { webReadioSdk } from "@/lib/readio-core-runtime";
import type { LocalePreference, MessageKey } from "@/lib/localization";
import type {
  EffortLevel,
  ReadingMode,
  ReadingPace,
} from "@/lib/rust-core";

interface SettingsDialogProps {
  open: boolean;
  onClose: () => void;
}

const NAV_ITEMS = [
  { id: "appearance", labelKey: "settings.appearance" },
  { id: "reading", labelKey: "settings.reading" },
  { id: "api-keys", labelKey: "settings.apiKeys" },
  { id: "diagnostics", labelKey: "settings.diagnostics" },
  { id: "about", labelKey: "settings.about" },
] as const;

type SectionId = (typeof NAV_ITEMS)[number]["id"];

/**
 * Full-screen modal overlay for settings.
 * Follows the same left-nav + right-content layout as Claude's settings dialog.
 * All settings are persisted to the backend via the useSettings hook.
 */
export function SettingsDialog({ open, onClose }: SettingsDialogProps) {
  const { t } = useLocale();
  const overlayRef = useRef<HTMLDivElement>(null);
  const { theme, setTheme } = useTheme();
  const [mounted, setMounted] = useState(false);
  const [activeSection, setActiveSection] = useState<SectionId>("appearance");
  const { settings, updateSetting } = useSettings();
  const [readingPace, setReadingPace] = useState<ReadingPace>({
    mode: "manual",
    effort: "high",
    multiplier: 1,
    text_characters_per_second: 46,
    audio_playback_rate: 1,
    advances_automatically: false,
    uses_audio: false,
  });

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

  useEffect(() => {
    let active = true;
    void resolveStoredReadingPace(settings).then((pace) => {
      if (active) setReadingPace(pace);
    });
    return () => {
      active = false;
    };
  }, [settings]);

  if (!open || !mounted) return null;

  // Derive current values from persisted settings (with sensible defaults)
  const accentIndex = parseInt(settings.accent_color ?? "0", 10);
  const fontSize = parseInt(settings.font_size ?? "18", 10);
  const storedBaseTextRate = Number(settings.reading_base_cps);
  const baseTextRate = Number.isFinite(storedBaseTextRate)
    ? storedBaseTextRate
    : 46;

  return createPortal(
    <div
      ref={overlayRef}
      className="ui-overlay"
      onClick={(e) => {
        if (e.target === overlayRef.current) onClose();
      }}
    >
      <div
        className="ui-modal ui-modal--settings"
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-dialog-title"
      >
        <div className="ui-modal-header">
          <h2 id="settings-dialog-title" className="ui-modal-title">{t("settings.title")}</h2>
          <button
            type="button"
            onClick={onClose}
            className="ui-icon-button"
            aria-label={t("settings.close")}
          >
            <X className="ui-icon" aria-hidden="true" />
          </button>
        </div>

        <div className="settings-layout">
          <nav className="settings-nav" aria-label={t("settings.sections")}>
            <ul>
              {NAV_ITEMS.map(({ id, labelKey }) => (
                <li key={id}>
                  <button
                    type="button"
                    onClick={() => setActiveSection(id)}
                    className="settings-nav-item"
                    data-active={activeSection === id}
                  >
                    {t(labelKey)}
                  </button>
                </li>
              ))}
            </ul>
          </nav>

          <div className="settings-content">
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
                mode={readingPace.mode}
                effort={readingPace.effort}
                baseTextRate={baseTextRate}
                pace={readingPace}
                onFontSizeChange={(size) => {
                  updateSetting("font_size", String(size));
                  applyFontSize(size);
                }}
                onModeChange={(mode) => {
                  updateSetting("reading_mode", mode);
                }}
                onEffortChange={(effort) => {
                  updateSetting("reading_effort", effort);
                }}
                onBaseTextRateChange={(rate) => {
                  updateSetting("reading_base_cps", String(rate));
                }}
              />
            )}
            {activeSection === "api-keys" && <ApiKeysSection />}
            {activeSection === "diagnostics" && <DiagnosticsSection />}
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
  const { preference, setPreference, t } = useLocale();
  const modes = [
    { value: "light", icon: Sun, labelKey: "settings.light" },
    { value: "dark", icon: Moon, labelKey: "settings.dark" },
    { value: "system", icon: Monitor, labelKey: "settings.system" },
  ] as const;
  const languages: Array<{ value: LocalePreference; labelKey: MessageKey }> = [
    { value: "system", labelKey: "settings.languageSystem" },
    { value: "en", labelKey: "settings.languageEnglish" },
    { value: "zh-Hans", labelKey: "settings.languageChinese" },
  ];

  return (
    <div className="space-y-8">
      <div>
        <SectionHeading>{t("settings.language")}</SectionHeading>
        <p className="text-sm text-text-tertiary mb-3">
          {t("settings.languageDescription")}
        </p>
        <select
          className="ui-input settings-field-input"
          value={preference}
          onChange={(event) =>
            setPreference(event.target.value as LocalePreference)
          }
          aria-label={t("settings.language")}
        >
          {languages.map(({ value, labelKey }) => (
            <option key={value} value={value}>
              {t(labelKey)}
            </option>
          ))}
        </select>
      </div>

      <Divider />

      <div>
        <SectionHeading>{t("settings.colorMode")}</SectionHeading>
        <div className="flex gap-3">
          {modes.map(({ value, icon: Icon, labelKey }) => (
            <button
              key={value}
              onClick={() => setTheme(value)}
              className="settings-choice"
              data-selected={theme === value}
            >
              <Icon
                className="ui-icon-lg"
                aria-hidden="true"
              />
              <span
                className={`text-xs font-medium ${theme === value ? "text-accent" : "text-text-secondary"}`}
              >
                {t(labelKey)}
              </span>
            </button>
          ))}
        </div>
      </div>

      <Divider />

      <div>
        <SectionHeading>{t("settings.accentColor")}</SectionHeading>
        <div className="flex gap-3">
          {ACCENT_PRESETS.map((preset, i) => (
            <button
              key={preset.name}
              onClick={() => onAccentChange(i)}
              title={preset.name}
              className="accent-swatch"
              data-selected={accentIndex === i}
              style={{
                backgroundColor: preset.light,
                color: preset.light,
              }}
            >
              {accentIndex === i && (
                <Check className="ui-icon-sm text-white" aria-hidden="true" />
              )}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

/* ─────────────── Section: Reading ─────────────── */

export function ReadingSection({
  fontSize,
  mode,
  effort,
  baseTextRate,
  pace,
  onFontSizeChange,
  onModeChange,
  onEffortChange,
  onBaseTextRateChange,
}: {
  fontSize: number;
  mode: ReadingMode;
  effort: EffortLevel;
  baseTextRate: number;
  pace: ReadingPace;
  onFontSizeChange: (size: number) => void;
  onModeChange: (mode: ReadingMode) => void;
  onEffortChange: (effort: EffortLevel) => void;
  onBaseTextRateChange: (rate: number) => void;
}) {
  const { t } = useLocale();
  const modes: Array<{ value: ReadingMode; labelKey: MessageKey }> = [
    { value: "manual", labelKey: "settings.manual" },
    { value: "auto", labelKey: "settings.auto" },
    { value: "read_aloud", labelKey: "settings.readAloud" },
  ];
  const effortLevels: Array<{ value: EffortLevel; labelKey: MessageKey }> = [
    { value: "minimal", labelKey: "settings.effortMinimal" },
    { value: "low", labelKey: "settings.effortLow" },
    { value: "medium", labelKey: "settings.effortMedium" },
    { value: "high", labelKey: "settings.effortHigh" },
    { value: "xhigh", labelKey: "settings.effortXHigh" },
    { value: "max", labelKey: "settings.effortMax" },
  ];

  return (
    <div className="space-y-8">
      <div>
        <SectionHeading>{t("settings.readingMode")}</SectionHeading>
        <p className="text-sm text-text-tertiary mb-3">
          {t("settings.readingModeDescription")}
        </p>
        <div className="segmented-control" aria-label={t("settings.readingMode")}>
          {modes.map((candidate) => (
            <button
              type="button"
              key={candidate.value}
              aria-pressed={mode === candidate.value}
              onClick={() => onModeChange(candidate.value)}
            >
              {t(candidate.labelKey)}
            </button>
          ))}
        </div>
      </div>

      <Divider />

      <div>
        <SectionHeading>{t("settings.readingEffort")}</SectionHeading>
        <p className="text-sm text-text-tertiary mb-3">
          {t("settings.readingEffortDescription")}
        </p>
        <div className="settings-field-row">
          <label className="settings-field-label" htmlFor="reading-effort">
            {t("settings.level")}
          </label>
          <select
            id="reading-effort"
            aria-label={t("settings.readingEffort")}
            className="ui-input settings-field-input"
            value={effort}
            onChange={(event) =>
              onEffortChange(event.target.value as EffortLevel)
            }
          >
            {effortLevels.map((level) => (
              <option key={level.value} value={level.value}>
                {t(level.labelKey)}
              </option>
            ))}
          </select>
        </div>
        <div className="settings-field-row mt-3">
          <label className="settings-field-label" htmlFor="reading-base-rate">
            {t("settings.baseTextPace")}
          </label>
          <input
            id="reading-base-rate"
            className="ui-input settings-field-input"
            type="number"
            min={4}
            max={4000}
            step={1}
            value={baseTextRate}
            onChange={(event) =>
              onBaseTextRateChange(Number(event.target.value))
            }
          />
          <span className="text-xs text-text-secondary">{t("settings.charactersPerSecond")}</span>
        </div>
        <p className="mt-3 text-sm text-text-secondary" aria-live="polite">
          {formatPace(pace.text_characters_per_second)} {t("settings.charactersPerSecond")} ·{" "}
          {formatPace(pace.audio_playback_rate)}× {t("settings.audio")}
        </p>
      </div>

      <Divider />

      <div>
        <SectionHeading>{t("settings.defaultFontSize")}</SectionHeading>
        <p className="text-sm text-text-tertiary mb-3">
          {t("settings.defaultFontSizeDescription")}
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
          <span className="settings-range-value">
            {fontSize}px
          </span>
        </div>
      </div>

    </div>
  );
}

function formatPace(value: number): string {
  return Number(value.toFixed(2)).toString();
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
  const { t } = useLocale();
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
        <SectionHeading>{t("settings.apiKeys")}</SectionHeading>
        <p className="text-sm text-text-tertiary">{t("common.loading")}</p>
      </div>
    );
  }

  return (
    <div className="space-y-8">
      <div>
        <SectionHeading>{t("settings.apiKeys")}</SectionHeading>
        <p className="text-sm text-text-tertiary mb-4">
          {t("settings.apiDescription")}
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
  const { showToast } = useToast();
  const { t } = useLocale();

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
    } catch {
      showToast("settings.failed", t("settings.couldNotSave", { label: config.label }));
    } finally {
      setSaving(false);
    }
  }

  async function handleRemove() {
    setSaving(true);
    try {
      await patchSettings({ [config.key]: null as unknown as string });
      onUpdate();
    } catch {
      showToast("settings.failed", t("settings.couldNotRemove", { label: config.label }));
    } finally {
      setSaving(false);
    }
  }

  if (editing) {
    return (
      <div className="settings-field-row">
        <label className="settings-field-label">{config.label}</label>
        <input
          type={config.isSecret && !showValue ? "password" : "text"}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          placeholder={config.placeholder}
          className="ui-input settings-field-input"
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
            className="ui-icon-button"
          >
            {showValue ? <EyeOff className="ui-icon-sm" /> : <Eye className="ui-icon-sm" />}
          </button>
        )}
        <button
          onClick={handleSave}
          disabled={saving || !value.trim()}
          className="ui-button ui-button--primary settings-compact-button"
        >
          {t("common.save")}
        </button>
        <button
          onClick={() => { setEditing(false); setValue(""); }}
          className="ui-button ui-button--quiet settings-compact-button"
        >
          {t("common.cancel")}
        </button>
      </div>
    );
  }

  return (
    <div className="settings-field-row">
      <label className="settings-field-label">{config.label}</label>
      <div className="settings-field-value" data-configured={isConfigured}>
        {isConfigured ? (
          <span className="text-text-primary font-mono text-xs">
            {status?.masked ?? t("settings.configured")}
          </span>
        ) : (
          <span className="text-text-tertiary text-xs">
            {t("settings.notSet")}
            {hasGlobalFallback && ` (${t("settings.usingServerDefault")})`}
          </span>
        )}
      </div>
      <button
        onClick={() => setEditing(true)}
        className="ui-button ui-button--secondary settings-compact-button"
      >
        {isConfigured ? t("settings.change") : t("settings.set")}
      </button>
      {isConfigured && (
        <button
          onClick={handleRemove}
          disabled={saving}
          className="ui-icon-button hover:text-red-500"
          title={t("settings.remove")}
        >
          <Trash2 className="ui-icon-sm" />
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
  const panelRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const { showToast } = useToast();
  const { t } = useLocale();

  const isConfigured = status?.configured ?? false;
  const currentValue = status?.masked ?? null;

  // Check if provider API key is available (for ElevenLabs voice browsing)
  const apiKeyField = `${config.provider}_api_key`;
  const hasApiKey = keyStatus[apiKeyField]?.configured || keyStatus[apiKeyField]?.has_global_fallback;

  // Close dropdown on outside click
  useEffect(() => {
    if (!expanded) return;
    const handler = (e: MouseEvent) => {
      const target = e.target as Node;
      if (
        !dropdownRef.current?.contains(target) &&
        !panelRef.current?.contains(target)
      ) {
        setExpanded(false);
        setSearch("");
      }
    };
    const keyHandler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setExpanded(false);
        setSearch("");
      }
    };
    document.addEventListener("mousedown", handler);
    document.addEventListener("keydown", keyHandler);
    return () => {
      document.removeEventListener("mousedown", handler);
      document.removeEventListener("keydown", keyHandler);
    };
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
      setError(e instanceof Error ? e.message : t("settings.failedToLoad"));
    } finally {
      setLoadingItems(false);
    }
  }

  function handleExpand() {
    if (config.fieldType === "voice" && config.provider === "elevenlabs" && !hasApiKey) {
      setError(t("settings.setApiKeyFirst"));
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
    } catch {
      showToast("settings.failed", t("settings.couldNotSave", { label: config.label }));
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
    } catch {
      showToast("settings.failed", t("settings.couldNotSave", { label: config.label }));
    } finally {
      setSaving(false);
    }
  }

  async function handleRemove() {
    setSaving(true);
    try {
      await patchSettings({ [config.key]: null as unknown as string });
      onUpdate();
    } catch {
      showToast("settings.failed", t("settings.couldNotRemove", { label: config.label }));
    } finally {
      setSaving(false);
    }
  }

  if (manualMode) {
    return (
      <div className="settings-field-row">
        <label className="settings-field-label">{config.label}</label>
        <input
          type="text"
          value={manualValue}
          onChange={(e) => setManualValue(e.target.value)}
          placeholder={config.placeholder}
          className="ui-input settings-field-input"
          autoFocus
          onKeyDown={(e) => {
            if (e.key === "Enter") handleManualSave();
            if (e.key === "Escape") { setManualMode(false); setManualValue(""); }
          }}
        />
        <button
          onClick={handleManualSave}
          disabled={saving || !manualValue.trim()}
          className="ui-button ui-button--primary settings-compact-button"
        >
          {t("common.save")}
        </button>
        <button
          onClick={() => { setManualMode(false); setManualValue(""); }}
          className="ui-button ui-button--quiet settings-compact-button"
        >
          {t("common.cancel")}
        </button>
      </div>
    );
  }

  const filtered = items.filter(
    (item) =>
      item.label.toLowerCase().includes(search.toLowerCase()) ||
      item.id.toLowerCase().includes(search.toLowerCase())
  );
  const anchorRect = dropdownRef.current?.getBoundingClientRect();

  return (
    <div className="settings-picker" ref={dropdownRef}>
      <div className="settings-field-row">
        <label className="settings-field-label">{config.label}</label>
        <div className="settings-field-value" data-configured={isConfigured}>
          {isConfigured ? (
            <span className="text-text-primary font-mono text-xs">{currentValue}</span>
          ) : (
            <span className="text-text-tertiary text-xs">
              {t("settings.notSet")}
              {status?.has_global_fallback && ` (${t("settings.usingServerDefault")})`}
            </span>
          )}
        </div>
        <button
          onClick={handleExpand}
          className="ui-button ui-button--secondary settings-compact-button"
        >
          <ChevronDown className="ui-icon-sm" />
          {isConfigured ? t("settings.change") : t("settings.select")}
        </button>
        {isConfigured && (
          <button
            onClick={handleRemove}
            disabled={saving}
            className="ui-icon-button hover:text-red-500"
            title={t("settings.remove")}
          >
            <Trash2 className="ui-icon-sm" />
          </button>
        )}
      </div>

      {expanded &&
        createPortal(
        <div
          ref={panelRef}
          className="surface-popover settings-picker-menu"
          style={{
            top: anchorRect
              ? `calc(${anchorRect.bottom}px + var(--space-1))`
              : "var(--space-16)",
            left: anchorRect?.left ?? "var(--space-4)",
            width: anchorRect?.width ?? "var(--modal-width-sm)",
          }}
        >
          {/* Search */}
          <div className="settings-picker-search">
            <Search className="ui-icon-sm text-text-tertiary" />
            <input
              ref={searchRef}
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder={t("settings.searchPlaceholder")}
              className="flex-1 text-sm bg-transparent text-text-primary placeholder:text-text-tertiary outline-none"
            />
          </div>

          {/* List */}
          <div className="settings-picker-list">
            {loadingItems && (
              <div className="settings-picker-state">{t("common.loading")}</div>
            )}
            {error && (
              <div className="settings-picker-state">{error}</div>
            )}
            {!loadingItems && !error && filtered.length === 0 && (
              <div className="settings-picker-state">{t("settings.noResults")}</div>
            )}
            {!loadingItems && !error && filtered.map((item) => (
              <button
                key={item.id}
                onClick={() => handleSelect(item.id)}
                disabled={saving}
                className="settings-picker-option"
              >
                <span className="text-sm text-text-primary flex items-center gap-2">
                  {item.label}
                  {item.id === defaultId && (
                    <span className="settings-default-badge">{t("settings.default")}</span>
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
              {t("settings.typeManually")}
            </button>
          </div>
        </div>,
        document.body
      )}
    </div>
  );
}

/* ─────────────── Section: Diagnostics ─────────────── */

async function downloadWebSupportBundle() {
  const bundle = await webDiagnostics.buildSupportBundle({
    core: webReadioSdk,
    appVersion: "0.1.0",
  });
  const file = supportBundleFile(bundle);
  const url = URL.createObjectURL(
    new Blob([file.contents], { type: file.mediaType })
  );
  try {
    const link = document.createElement("a");
    link.href = url;
    link.download = file.filename;
    link.click();
  } finally {
    URL.revokeObjectURL(url);
  }
}

export function DiagnosticsSection({
  onDownload = downloadWebSupportBundle,
}: {
  onDownload?: () => Promise<void>;
} = {}) {
  const { t } = useLocale();
  const [status, setStatus] = useState<"idle" | "downloading" | "done" | "failed">("idle");

  async function handleDownload() {
    setStatus("downloading");
    try {
      await onDownload();
      setStatus("done");
    } catch {
      setStatus("failed");
    }
  }

  return (
    <div className="space-y-6">
      <div>
        <SectionHeading>{t("settings.diagnostics")}</SectionHeading>
        <p className="text-sm text-text-secondary leading-relaxed">
          {t("settings.diagnosticsDescription")}
        </p>
      </div>
      <div className="flex items-start gap-3">
        <ShieldCheck className="ui-icon text-accent mt-0.5" aria-hidden="true" />
        <p className="text-sm text-text-secondary leading-relaxed">
          {t("settings.diagnosticsPrivacy")}
        </p>
      </div>
      <button
        type="button"
        className="ui-button ui-button--primary"
        disabled={status === "downloading"}
        onClick={() => void handleDownload()}
      >
        <Download className="ui-icon-sm" aria-hidden="true" />
        {status === "downloading"
          ? t("settings.diagnosticsDownloading")
          : t("settings.diagnosticsDownload")}
      </button>
      <p className="text-sm text-text-secondary" role="status" aria-live="polite">
        {status === "done"
          ? t("settings.diagnosticsDownloaded")
          : status === "failed"
            ? t("settings.diagnosticsFailed")
            : ""}
      </p>
    </div>
  );
}

/* ─────────────── Section: About ─────────────── */

function AboutSection() {
  const { t } = useLocale();
  return (
    <div className="space-y-4">
      <SectionHeading>{t("settings.about")}</SectionHeading>
      <div className="space-y-3">
        <div className="flex items-center gap-3">
          <BrandMark />
          <span className="text-xs text-text-tertiary">v0.1.0</span>
        </div>
        <p className="text-sm text-text-secondary leading-relaxed">
          {t("settings.aboutDescription")}
        </p>
        <a
          href="https://github.com/nicepkg/readio"
          target="_blank"
          rel="noopener noreferrer"
          className="inline-block text-sm text-accent hover:underline"
        >
          {t("settings.viewOnGitHub")}
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
