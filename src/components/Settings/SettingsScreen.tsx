import { UpdateSettings } from "../Updates";
import { useEffect, useRef, useState, type CSSProperties, type ElementType, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { useMixerStore } from "../../store/mixer";
import {
  DEFAULT_SHORTCUTS,
  useShortcutSettings,
  type ShortcutAction,
} from "../../store/shortcuts";
import { useTheme, THEMES } from "../../store/theme";
import { OSD_STYLES, useOsdStyle, type OsdStyle } from "../../store/osdStyle";
import { OSD_POSITIONS, useOsdPosition, type OsdPosition } from "../../store/osdPosition";
import { restartApplication, showOsd } from "../../hooks/useGlobalShortcuts";
import { stashRestoreWarning } from "../../lib/restoreWarning";
import { useI18n, type TranslationKey } from "../../i18n";
import { reloadLanguagePacks, type LanguagePackCatalog } from "../../languagePacks";
import type { MeterMode, OutputDevice, ProfileAutomationConfig } from "../../types";
import { Ms } from "../Icons";
import { ConfirmModal } from "../ConfirmModal";
import { HelpInfo } from "../HelpInfo";
import { MenuItem } from "../MenuItem";
import { Popover } from "../Popover";
import { ProcessingInfo } from "../ProcessingInfo";
import { ShortcutRecorderInput } from "../ShortcutRecorderInput";
import { Toggle } from "../Toggle";

interface DefaultDevices {
  output: string | null;
  input: string | null;
}

interface BackupStatus {
  count: number;
  last_backup_at: number | null;
}

interface RestoreBackupResult {
  frontend_state: Record<string, string>;
  recovery_backup: string;
  warning: string | null;
}

const BACKUP_FRONTEND_KEYS = [
  "mixweave-theme",
  "mixweave-language",
  "mixweave-global-shortcuts",
  "mixweave-active-eq-presets",
  "mixweave-profile-section-visibility",
] as const;

// A backup made before the Mixweave rename stored these same values under
// their old "sonux-" key names - read as a fallback when exporting (so a
// value never re-saved since upgrading is still captured) and accepted when
// restoring an older backup file.
const LEGACY_BACKUP_FRONTEND_KEYS: Record<(typeof BACKUP_FRONTEND_KEYS)[number], string> = {
  "mixweave-theme": "sonux-theme",
  "mixweave-language": "sonux-language",
  "mixweave-global-shortcuts": "sonux-global-shortcuts",
  "mixweave-active-eq-presets": "sonux-active-eq-presets",
  "mixweave-profile-section-visibility": "sonux-profile-section-visibility",
};

function frontendBackupState(): Record<string, string> {
  return Object.fromEntries(BACKUP_FRONTEND_KEYS.flatMap((key) => {
    const value = localStorage.getItem(key) ?? localStorage.getItem(LEGACY_BACKUP_FRONTEND_KEYS[key]);
    return value === null ? [] : [[key, value]];
  }));
}

function applyFrontendBackupState(state: Record<string, string>) {
  for (const key of BACKUP_FRONTEND_KEYS) {
    localStorage.removeItem(key);
    localStorage.removeItem(LEGACY_BACKUP_FRONTEND_KEYS[key]);
  }
  for (const [key, value] of Object.entries(state)) {
    if ((BACKUP_FRONTEND_KEYS as readonly string[]).includes(key)) {
      localStorage.setItem(key, value);
      continue;
    }
    const current = (Object.keys(LEGACY_BACKUP_FRONTEND_KEYS) as (typeof BACKUP_FRONTEND_KEYS)[number][])
      .find((mapped) => LEGACY_BACKUP_FRONTEND_KEYS[mapped] === key);
    if (current) localStorage.setItem(current, value);
  }
}

function backupStatusText(status: BackupStatus | null, locale: string, t: ReturnType<typeof useI18n>["t"]): string {
  if (!status) return t("settings.backups.checking");
  if (status.count === 0 || status.last_backup_at === null) return t("settings.backups.none");
  const count = t(status.count === 1 ? "settings.backups.countOne" : "settings.backups.countMany", { count: status.count });
  const created = new Date(status.last_backup_at * 1000);
  const date = new Intl.DateTimeFormat(locale, {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(created);
  const time = new Intl.DateTimeFormat(locale, {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(created);
  return t("settings.backups.last", { count, date, time });
}

function fileName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

const METER_MODES: { value: MeterMode; label: TranslationKey; detail: TranslationKey }[] = [
  { value: "monitor", label: "settings.meters.monitor.label", detail: "settings.meters.monitor.detail" },
  { value: "fps_144", label: "settings.meters.fps144.label", detail: "settings.meters.fps144.detail" },
  { value: "fps_120", label: "settings.meters.fps120.label", detail: "settings.meters.fps120.detail" },
  { value: "fps_100", label: "settings.meters.fps100.label", detail: "settings.meters.fps100.detail" },
  { value: "fps_60", label: "settings.meters.fps60.label", detail: "settings.meters.fps60.detail" },
  { value: "off", label: "settings.meters.off.label", detail: "settings.meters.off.detail" },
];

const SHORTCUT_ROWS: { action: ShortcutAction; label: TranslationKey; icon: string }[] = [
  { action: "toggle_game", label: "settings.shortcuts.game", icon: "sports_esports" },
  { action: "toggle_chat", label: "settings.shortcuts.chat", icon: "forum" },
  { action: "toggle_mic", label: "settings.shortcuts.microphone", icon: "mic" },
  { action: "restart_app", label: "settings.shortcuts.restart", icon: "restart_alt" },
];

/** One titled card of settings. */
function SettingsSection({ title, children, preserve = false }: Readonly<{ title: string; children: ReactNode; preserve?: boolean }>) {
  return (
    <section className={"settings-section" + (preserve ? " preserve-settings" : "")}>
      <div className="section-head">
        <h2 className="section-title">{title}</h2>
      </div>
      <div className="card settings-card">{children}</div>
    </section>
  );
}

/** A setting: icon tile, title with its explanation underneath, control on the right.
 * `stacked` puts a wide control (like the theme picker) under the text instead of beside it. */
function SettingRow({
  icon,
  title,
  description,
  help,
  sub = false,
  disabled = false,
  stacked = false,
  as: Tag = "div",
  className,
  titleId,
  children,
}: Readonly<{
  icon: string;
  title: string;
  description?: string;
  /** Extra info button shown next to the title. */
  help?: ReactNode;
  /** A setting that only applies while the one above it is on. */
  sub?: boolean;
  disabled?: boolean;
  stacked?: boolean;
  as?: ElementType;
  className?: string;
  titleId?: string;
  children?: ReactNode;
}>) {
  const classes = ["setting-row", sub && "sub", disabled && "disabled", stacked && "stacked", className]
    .filter(Boolean)
    .join(" ");
  return (
    <Tag className={classes}>
      <span className="setting-icon">
        <Ms name={icon} />
      </span>
      <div className="setting-copy">
        <div className="setting-title">
          <span id={titleId}>{title}</span>
          {help}
        </div>
        {description && <div className="setting-desc">{description}</div>}
      </div>
      {children != null && <div className="setting-control">{children}</div>}
    </Tag>
  );
}

/** Miniature mixer drawn in one theme's real palette, for the theme picker. */
function ThemeMock({ tone, split = false }: Readonly<{ tone: "original" | "dark"; split?: boolean }>) {
  return (
    <span className={"theme-mock" + (split ? " split" : "")} data-tone={tone}>
      <span className="theme-mock-side"><i /><i /><i /></span>
      <span className="theme-mock-main">
        {[62, 38, 78].map((level) => (
          <span key={level} className="theme-mock-strip">
            <span className="theme-mock-track" style={{ "--level": `${level}%` } as CSSProperties}>
              <b />
            </span>
          </span>
        ))}
      </span>
    </span>
  );
}

/** A screen silhouette with the popup drawn at its physical screen position. */
function OsdPositionIcon({ position }: Readonly<{ position: OsdPosition }>) {
  const x = position.endsWith("right") ? 23 : 7;
  const y = position.startsWith("top") ? 7 : position.startsWith("bottom") ? 19 : 13;
  return (
    <svg className="osd-position-icon" width="40" height="32" viewBox="0 0 40 32" fill="none" aria-hidden="true">
      <rect className="osd-position-screen" x="2" y="2" width="36" height="26" rx="5" stroke="currentColor" strokeWidth="1.5" />
      <path d="M16 31h8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" opacity="0.4" />
      <rect x={x} y={y} width="10" height="5" rx="2" fill="currentColor" />
    </svg>
  );
}

/** Miniature of one shortcut-popup look, for the picker. */
function OsdMini({ style }: Readonly<{ style: OsdStyle }>) {
  const count = style === "segments" ? 14 : style === "waves" ? 16 : 0;
  return (
    <span className={`osd-mini ${style}`} aria-hidden="true">
      {style === "fader" ? (
        <>
          <b className="mini-dot" />
          <b className="mini-rail"><b className="mini-fill" /><b className="mini-knob" /></b>
          <b className="mini-num" />
        </>
      ) : (
        <>
          <b className="mini-top"><b className="mini-tile" /><b className="mini-num" /></b>
          <b className="mini-bars">
            {Array.from({ length: count }, (_, i) => (
              <i key={i} className={i < Math.round(count * 0.72) ? "on" : undefined} style={{ "--h": `${style === "waves" ? 35 + ((i * 37) % 60) : 100}%` } as CSSProperties} />
            ))}
          </b>
        </>
      )}
    </span>
  );
}

/** Card row with a device dropdown for picking a system default. */
function DeviceRow({
  icon,
  title,
  sub,
  devices,
  current,
  onPick,
}: Readonly<{
  icon: string;
  title: string;
  /** What this default is used for. */
  sub: string;
  devices: OutputDevice[];
  current: string | null;
  onPick: (name: string) => void;
}>) {
  const [open, setOpen] = useState(false);
  const currentDesc = devices.find((d) => d.name === current)?.description ?? current ?? "-";

  return (
    <SettingRow icon={icon} title={title} description={sub}>
      <div style={{ position: "relative" }}>
        <button type="button" className="select device-select" onClick={() => setOpen((o) => !o)}>
          <span className="device-select-name" title={currentDesc}>{currentDesc}</span>
          <Ms name="expand_more" />
        </button>
        <Popover open={open} onClose={() => setOpen(false)} side="bottom" align="end">
          {devices.map((d) => (
            <MenuItem
              key={d.name}
              icon={icon}
              selected={d.name === current}
              showCheck
              onClick={() => {
                onPick(d.name);
                setOpen(false);
              }}
            >
              {d.description}
            </MenuItem>
          ))}
        </Popover>
      </div>
    </SettingRow>
  );
}

export function SettingsScreen() {
  const { availableLocales, locale, systemLocale, preference, setPreference, t } = useI18n();
  const { theme, setTheme } = useTheme();
  const osdStyle = useOsdStyle((state) => state.style);
  const setOsdStyle = useOsdStyle((state) => state.setStyle);
  const osdPosition = useOsdPosition((state) => state.position);
  const setOsdPosition = useOsdPosition((state) => state.setPosition);
  const [osdPositionOpen, setOsdPositionOpen] = useState(false);
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [startMinimized, setStartMinimized] = useState<boolean | null>(null);
  const [startupSaving, setStartupSaving] = useState(false);
  const startupSavingRef = useRef(false);
  const [version, setVersion] = useState("");
  const [defaults, setDefaults] = useState<DefaultDevices>({ output: null, input: null });
  const [languageOpen, setLanguageOpen] = useState(false);
  const [returnOpen, setReturnOpen] = useState(false);
  const [meterModeOpen, setMeterModeOpen] = useState(false);
  const [profileAutomation, setProfileAutomation] = useState<ProfileAutomationConfig | null>(null);
  const [confirmingReset, setConfirmingReset] = useState(false);
  const [backupStatus, setBackupStatus] = useState<BackupStatus | null>(null);
  const [backupBusy, setBackupBusy] = useState<"create" | "restore" | "open" | null>(null);
  const [restorePath, setRestorePath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const outputDevices = useMixerStore((s) => s.outputDevices);
  const inputDevices = useMixerStore((s) => s.inputDevices);
  const profiles = useMixerStore((s) => s.profiles);
  const replayOnboarding = useMixerStore((s) => s.replayOnboarding);
  const showBalance = useMixerStore((s) => s.showBalance);
  const setBalanceVisible = useMixerStore((s) => s.setBalanceVisible);
  const meterMode = useMixerStore((s) => s.meterMode);
  const setMeterMode = useMixerStore((s) => s.setMeterMode);
  const shortcutsEnabled = useShortcutSettings((s) => s.enabled);
  const shortcutBindings = useShortcutSettings((s) => s.bindings);
  const setShortcutsEnabled = useShortcutSettings((s) => s.setEnabled);
  const setShortcutBindings = useShortcutSettings((s) => s.setBindings);
  const [recordingShortcut, setRecordingShortcut] = useState<ShortcutAction | null>(null);
  const [languageCatalog, setLanguageCatalog] = useState<LanguagePackCatalog | null>(null);

  useEffect(() => {
    void invoke<boolean>("get_autostart").then(setAutostart).catch((reason) => setError(String(reason)));
    void invoke<DefaultDevices>("get_default_devices").then(setDefaults).catch(() => {});
    void invoke<ProfileAutomationConfig>("get_profile_automation").then(setProfileAutomation).catch(() => {});
    void invoke<BackupStatus>("get_backup_status").then(setBackupStatus).catch(() => {});
    void invoke<{ start_minimized: boolean }>("get_prefs")
      .then((p) => {
        setStartMinimized(p.start_minimized);
      })
      .catch(() => {});
    void getVersion().then(setVersion);
    void reloadLanguagePacks().then(setLanguageCatalog).catch((reason) => setError(String(reason)));
  }, []);

  const pickDefault = async (kind: "output" | "input", name: string) => {
    try {
      await invoke(kind === "output" ? "set_default_output" : "set_default_input", { name });
      setDefaults((d) => ({ ...d, [kind]: name }));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const toggleAutostart = async () => {
    if (autostart === null || startupSavingRef.current) return;
    startupSavingRef.current = true;
    setStartupSaving(true);
    setAutostart(!autostart);
    try {
      const actual = await invoke<boolean>("set_autostart", { enabled: !autostart });
      setAutostart(actual);
      setError(null);
    } catch (e) {
      setAutostart(autostart);
      setError(String(e));
    } finally {
      startupSavingRef.current = false;
      setStartupSaving(false);
    }
  };

  const toggleStartMinimized = async () => {
    if (startMinimized === null || startupSavingRef.current) return;
    startupSavingRef.current = true;
    setStartupSaving(true);
    const next = !startMinimized;
    setStartMinimized(next);
    try {
      await invoke("set_start_minimized", { minimized: next });
      setError(null);
    } catch (e) {
      setStartMinimized(!next);
      setError(String(e));
    } finally {
      startupSavingRef.current = false;
      setStartupSaving(false);
    }
  };

  const saveProfileAutomation = async (next: ProfileAutomationConfig) => {
    try {
      await invoke("save_profile_automation", { config: next });
      setProfileAutomation(next);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const createBackup = async () => {
    setBackupBusy("create");
    try {
      const status = await invoke<BackupStatus>("create_backup", {
        frontendState: frontendBackupState(),
      });
      setBackupStatus(status);
      setError(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBackupBusy(null);
    }
  };

  const chooseBackup = async () => {
    try {
      const selected = await invoke<string | null>("choose_backup_for_restore");
      if (selected) setRestorePath(selected);
    } catch (reason) {
      setError(String(reason));
    }
  };

  const restoreBackup = async () => {
    setBackupBusy("restore");
    try {
      const restored = await invoke<RestoreBackupResult>("restore_backup", {
        frontendState: frontendBackupState(),
      });
      applyFrontendBackupState(restored.frontend_state);
      // A peripheral restore warning must not leave the old in-memory mixer
      // running against the newly replaced config tree. Carry the warning
      // across the mandatory restart and show it in the fresh process.
      stashRestoreWarning(restored.warning);
      await invoke("restart_app");
    } catch (reason) {
      setError(String(reason));
      setBackupBusy(null);
      void invoke<BackupStatus>("get_backup_status").then(setBackupStatus).catch(() => {});
    }
  };

  const openBackupLocation = async () => {
    setBackupBusy("open");
    try {
      await invoke("open_backup_location");
      setError(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBackupBusy(null);
    }
  };

  const selectedLanguageValue = preference.mode === "system"
    ? "system"
    : availableLocales.some((candidate) => candidate.locale === preference.locale)
      ? preference.locale
      : locale !== "en" ? locale : preference.locale;

  const languageLabel = selectedLanguageValue === "system"
    ? t("settings.language.systemResolved", {
      language: availableLocales.find((candidate) => candidate.locale === systemLocale)?.nativeName ?? "English",
    })
    : availableLocales.find((candidate) => candidate.locale === selectedLanguageValue)?.nativeName
      ?? t("settings.language.unavailableOption", { locale: selectedLanguageValue });
  const meterOption = METER_MODES.find((option) => option.value === meterMode);
  const meterDescription = `${t("settings.meters.description", {
    detail: t(meterOption?.detail ?? "settings.meters.off.detail"),
  })}${meterMode !== "off" ? ` ${t("settings.meters.cpuHint")}` : ""}`;

  return (
    <div className="content narrow settings-screen">
      <div className="screen-head screen-head-rich">
        <span className="head-icon"><Ms name="settings" /></span>
        <div className="head-copy">
          <h1>{t("settings.title")}</h1>
          <p className="head-sub">{t("settings.subtitle")}</p>
        </div>
      </div>
      <div className="screen-scroll">
        {error && <div className="error-banner" style={{ borderRadius: 8 }}>{error}</div>}

        <SettingsSection title={t("settings.appearance.section")}>
          <SettingRow className="preserve-setting" icon="palette" title={t("settings.theme.title")} description={t("settings.theme.description")} stacked>
            <div className="theme-picker" role="radiogroup" aria-label={t("settings.theme.title")}>
              {THEMES.map((themeOption, index) => (
                <button
                  key={themeOption.id}
                  type="button"
                  role="radio"
                  aria-checked={themeOption.id === theme}
                  tabIndex={themeOption.id === theme ? 0 : -1}
                  className={"theme-card" + (themeOption.id === theme ? " active" : "")}
                  onClick={() => setTheme(themeOption.id)}
                  onKeyDown={(event) => {
                    const flip = document.documentElement.dir === "rtl" ? -1 : 1;
                    const step = ({ ArrowRight: flip, ArrowLeft: -flip, ArrowDown: 1, ArrowUp: -1 } as Record<string, number>)[event.key];
                    if (!step) return;
                    event.preventDefault();
                    const next = THEMES[(index + step + THEMES.length) % THEMES.length];
                    setTheme(next.id);
                    (event.currentTarget.parentElement?.children[THEMES.indexOf(next)] as HTMLElement | undefined)?.focus();
                  }}
                  title={t(themeOption.labelKey)}
                >
                  <span className="theme-preview" aria-hidden="true">
                    {themeOption.id === "system" ? (
                      <>
                        <ThemeMock tone="original" />
                        <ThemeMock tone="dark" split />
                      </>
                    ) : (
                      <ThemeMock tone={themeOption.id} />
                    )}
                    <span className="theme-check"><Ms name="check" /></span>
                  </span>
                  <span className="theme-card-label">{t(themeOption.labelKey)}</span>
                </button>
              ))}
            </div>
          </SettingRow>

          <SettingRow
            icon="translate"
            title={t("settings.language.title")}
            description={t("settings.language.description")}
            titleId="language-setting-title"
            help={languageCatalog && languageCatalog.warnings.length > 0 ? (
              <ProcessingInfo
                label={t("settings.language.warningDetails")}
                text={languageCatalog.warnings.join("\n\n")}
              />
            ) : undefined}
          >
            <div style={{ position: "relative" }}>
              <button
                type="button"
                className="select automation-settings-select"
                aria-labelledby="language-setting-title language-current-value"
                aria-haspopup="menu"
                aria-expanded={languageOpen}
                aria-controls="language-menu"
                onClick={() => setLanguageOpen((open) => !open)}
                onKeyDown={(event) => {
                  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                    event.preventDefault();
                    setLanguageOpen(true);
                  }
                }}
              >
                <span id="language-current-value" title={languageLabel}>{languageLabel}</span>
                <Ms name="expand_more" />
              </button>
              <Popover id="language-menu" open={languageOpen} onClose={() => setLanguageOpen(false)} side="bottom" align="end" style={{ minWidth: 240 }}>
                <MenuItem
                  selected={preference.mode === "system"}
                  showCheck
                  onClick={() => {
                    setPreference({ mode: "system" });
                    setLanguageOpen(false);
                  }}
                >
                  {t("settings.language.systemResolved", {
                    language: availableLocales.find((candidate) => candidate.locale === systemLocale)?.nativeName ?? "English",
                  })}
                </MenuItem>
                {availableLocales.map((candidate) => (
                  <MenuItem
                    key={candidate.locale}
                    selected={preference.mode === "locale" && selectedLanguageValue === candidate.locale}
                    showCheck
                    onClick={() => {
                      setPreference({ mode: "locale", locale: candidate.locale });
                      setLanguageOpen(false);
                    }}
                  >
                    {candidate.nativeName}
                  </MenuItem>
                ))}
              </Popover>
            </div>
          </SettingRow>

          <SettingRow icon="speed" title={t("settings.meters.title")} description={meterDescription}>
            <div style={{ position: "relative" }}>
              <button type="button" className="select" onClick={() => setMeterModeOpen((open) => !open)}>
                <span>{t(meterOption?.label ?? "settings.meters.off.label")}</span>
                <Ms name="expand_more" />
              </button>
              <Popover open={meterModeOpen} onClose={() => setMeterModeOpen(false)} side="bottom" align="end">
                {METER_MODES.map((option) => (
                  <MenuItem
                    key={option.value}
                    selected={option.value === meterMode}
                    showCheck
                    onClick={() => {
                      void setMeterMode(option.value);
                      setMeterModeOpen(false);
                    }}
                  >
                    {t(option.label)}
                  </MenuItem>
                ))}
              </Popover>
            </div>
          </SettingRow>
        </SettingsSection>

        <SettingsSection title={t("settings.overlay.section")} preserve>
          <SettingRow icon="notifications_active" title={t("settings.osd.title")} description={t("settings.osd.description")} stacked>
            <div className="theme-picker osd-picker" role="radiogroup" aria-label={t("settings.osd.title")}>
              {OSD_STYLES.map((option, index) => (
                <button
                  key={option.id}
                  type="button"
                  role="radio"
                  aria-checked={option.id === osdStyle}
                  tabIndex={option.id === osdStyle ? 0 : -1}
                  className={"theme-card" + (option.id === osdStyle ? " active" : "")}
                  onClick={() => {
                    setOsdStyle(option.id);
                    // Show it once, so the choice can be judged on screen.
                    showOsd({ label: t("settings.osd.sample"), volumePercent: 72, max: 100, muted: false });
                  }}
                  onKeyDown={(event) => {
                    const flip = document.documentElement.dir === "rtl" ? -1 : 1;
                    const step = ({ ArrowRight: flip, ArrowLeft: -flip, ArrowDown: 1, ArrowUp: -1 } as Record<string, number>)[event.key];
                    if (!step) return;
                    event.preventDefault();
                    const next = OSD_STYLES[(index + step + OSD_STYLES.length) % OSD_STYLES.length];
                    setOsdStyle(next.id);
                    (event.currentTarget.parentElement?.children[OSD_STYLES.indexOf(next)] as HTMLElement | undefined)?.focus();
                  }}
                  title={t(option.labelKey)}
                >
                  <span className="theme-preview" aria-hidden="true">
                    <OsdMini style={option.id} />
                    <span className="theme-check"><Ms name="check" /></span>
                  </span>
                  <span className="theme-card-label">{t(option.labelKey)}</span>
                </button>
              ))}
            </div>
          </SettingRow>

          <SettingRow icon="my_location" title={t("settings.osd.position.title")} description={t("settings.osd.position.description")}>
            <div style={{ position: "relative" }}>
              <button
                type="button"
                className="select osd-position-select"
                aria-label={`${t("settings.osd.position.title")}: ${t(OSD_POSITIONS.find((option) => option.id === osdPosition)?.labelKey ?? "settings.osd.position.middleRight")}`}
                aria-haspopup="menu"
                aria-expanded={osdPositionOpen}
                aria-controls={osdPositionOpen ? "osd-position-menu" : undefined}
                onClick={() => setOsdPositionOpen((open) => !open)}
                onKeyDown={(event) => {
                  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                    event.preventDefault();
                    setOsdPositionOpen(true);
                  }
                }}
              >
                <OsdPositionIcon position={osdPosition} />
                <span>{t(OSD_POSITIONS.find((option) => option.id === osdPosition)?.labelKey ?? "settings.osd.position.middleRight")}</span>
                <Ms name="expand_more" />
              </button>
              <Popover id="osd-position-menu" open={osdPositionOpen} onClose={() => setOsdPositionOpen(false)} side="bottom" align="end" style={{ width: 260, maxWidth: "calc(100vw - 16px)", padding: 6 }}>
                {OSD_POSITIONS.map((option) => (
                  <MenuItem
                    key={option.id}
                    className="osd-position-option"
                    selected={option.id === osdPosition}
                    showCheck
                    onClick={() => {
                      setOsdPosition(option.id);
                      setOsdPositionOpen(false);
                      // Show it once, so the new spot can be judged on screen.
                      showOsd({ label: t("settings.osd.sample"), volumePercent: 72, max: 100, muted: false });
                    }}
                  >
                    <OsdPositionIcon position={option.id} />
                    <span>{t(option.labelKey)}</span>
                  </MenuItem>
                ))}
              </Popover>
            </div>
          </SettingRow>
        </SettingsSection>

        <SettingsSection title={t("settings.startup.section")}>
          <SettingRow
            icon="rocket_launch"
            title={t("settings.autostart.title")}
            description={t("settings.autostart.description")}
          >
            {autostart !== null && <Toggle on={autostart} disabled={startupSaving} onClick={() => void toggleAutostart()} />}
          </SettingRow>
          {autostart && (
            <SettingRow
              sub
              icon="dock_to_bottom"
              title={t("settings.autostart.minimized.title")}
              description={t("settings.autostart.minimized.description")}
            >
              <Toggle on={startMinimized ?? false} disabled={startMinimized === null || startupSaving} onClick={() => void toggleStartMinimized()} />
            </SettingRow>
          )}
        </SettingsSection>

        <SettingsSection title={t("settings.preferences.section")}>
          <DeviceRow
            icon="speaker"
            title={t("settings.defaults.output.title")}
            sub={t("settings.defaults.output.description")}
            devices={outputDevices}
            current={defaults.output}
            onPick={(name) => void pickDefault("output", name)}
          />
          <DeviceRow
            icon="mic"
            title={t("settings.defaults.input.title")}
            sub={t("settings.defaults.input.description")}
            devices={inputDevices}
            current={defaults.input}
            onPick={(name) => void pickDefault("input", name)}
          />
          <SettingRow
            icon="balance"
            title={t("settings.balance.title")}
            description={t("settings.balance.description")}
          >
            <Toggle on={showBalance} onClick={() => void setBalanceVisible(!showBalance)} />
          </SettingRow>
        </SettingsSection>

        <SettingsSection title={t("settings.automation.section")}>
          <SettingRow
            icon="automation"
            title={t("settings.automation.enable.title")}
            description={t("settings.automation.enable.description")}
            help={(
              <HelpInfo
                label={t("settings.automation.info.label")}
                text={t("settings.automation.info.text")}
              />
            )}
          >
            {profileAutomation && (
              <Toggle
                on={profileAutomation.enabled}
                onClick={() => void saveProfileAutomation({ ...profileAutomation, enabled: !profileAutomation.enabled })}
              />
            )}
          </SettingRow>
          <SettingRow
            sub
            icon="restore_page"
            title={t("settings.automation.return.title")}
            description={t("settings.automation.return.description")}
            disabled={!profileAutomation?.enabled}
          >
            {profileAutomation && (
              <div style={{ position: "relative" }}>
                <button
                  type="button"
                  className="select automation-settings-select"
                  disabled={!profileAutomation.enabled}
                  aria-label={t("settings.automation.return.title")}
                  aria-haspopup="menu"
                  aria-expanded={returnOpen}
                  aria-controls="automation-return-menu"
                  onClick={() => setReturnOpen((open) => !open)}
                >
                  <span title={profileAutomation.return_profile
                    ? t("settings.automation.return.named", { profile: profileAutomation.return_profile })
                    : t("settings.automation.return.previous")}>{profileAutomation.return_profile
                    ? t("settings.automation.return.named", { profile: profileAutomation.return_profile })
                    : t("settings.automation.return.previous")}</span>
                  <Ms name="expand_more" />
                </button>
                <Popover id="automation-return-menu" open={returnOpen} onClose={() => setReturnOpen(false)} side="bottom" align="end" style={{ minWidth: 240 }}>
                  <MenuItem
                    selected={!profileAutomation.return_profile}
                    showCheck
                    onClick={() => {
                      void saveProfileAutomation({ ...profileAutomation, return_profile: null });
                      setReturnOpen(false);
                    }}
                  >
                    {t("settings.automation.return.previous")}
                  </MenuItem>
                  {profiles.map((profile) => (
                    <MenuItem
                      key={profile.name}
                      selected={profileAutomation.return_profile === profile.name}
                      showCheck
                      onClick={() => {
                        void saveProfileAutomation({ ...profileAutomation, return_profile: profile.name });
                        setReturnOpen(false);
                      }}
                    >
                      {t("settings.automation.return.named", { profile: profile.name })}
                    </MenuItem>
                  ))}
                </Popover>
              </div>
            )}
          </SettingRow>
          <SettingRow
            sub
            icon="notifications"
            title={t("settings.automation.notifications.title")}
            description={t("settings.automation.notifications.description")}
            disabled={!profileAutomation?.enabled}
          >
            {profileAutomation && (
              <Toggle
                on={profileAutomation.notifications}
                disabled={!profileAutomation.enabled}
                onClick={() => void saveProfileAutomation({
                  ...profileAutomation,
                  notifications: !profileAutomation.notifications,
                })}
              />
            )}
          </SettingRow>
        </SettingsSection>

        <SettingsSection title={t("settings.shortcuts.section")}>
          <SettingRow
            icon="keyboard"
            title={t("settings.shortcuts.enable.title")}
            description={t("settings.shortcuts.enable.description")}
            help={(
              <HelpInfo
                label={t("settings.shortcuts.info.label")}
                text={t("settings.shortcuts.info.text")}
              />
            )}
          >
            <Toggle on={shortcutsEnabled} onClick={() => setShortcutsEnabled(!shortcutsEnabled)} />
          </SettingRow>
          {SHORTCUT_ROWS.map(({ action, label, icon }) => (
            <SettingRow sub as="label" className="shortcut-row" key={action} icon={icon} title={t(label)} disabled={!shortcutsEnabled}>
              <ShortcutRecorderInput
                value={shortcutBindings[action]}
                recording={recordingShortcut === action}
                idleAriaLabel={t("settings.shortcuts.label", { label: t(label) })}
                recordingAriaLabel={t("settings.shortcuts.recordingLabel", { label: t(label) })}
                recordingLabel={t("settings.shortcuts.recording")}
                placeholder={t("settings.shortcuts.placeholder")}
                title={t("settings.shortcuts.inputHint")}
                onStartRecording={() => setRecordingShortcut(action)}
                onStopRecording={() => setRecordingShortcut((current) => (current === action ? null : current))}
                onCapture={(shortcut) => setShortcutBindings({ ...shortcutBindings, [action]: shortcut })}
              />
            </SettingRow>
          ))}
          <div className="setting-footer">
            <button
              type="button"
              className="select"
              disabled={JSON.stringify(shortcutBindings) === JSON.stringify(DEFAULT_SHORTCUTS)}
              onClick={() => setShortcutBindings({ ...DEFAULT_SHORTCUTS })}
            >
              {t("settings.shortcuts.restoreDefaults")}
            </button>
          </div>
        </SettingsSection>

        <SettingsSection title={t("settings.backups.section")}>
          <SettingRow
            icon="backup"
            title={t("settings.backups.manual.title")}
            description={backupStatusText(backupStatus, locale, t)}
          >
            <button
              type="button"
              className="select"
              disabled={backupBusy !== null}
              onClick={() => void createBackup()}
            >
              <span>{t(backupBusy === "create" ? "settings.backups.creating" : "settings.backups.create")}</span>
            </button>
          </SettingRow>
          <SettingRow
            icon="folder_open"
            title={t("settings.backups.location.title")}
            description={t("settings.backups.location.description")}
          >
            <button
              type="button"
              className="select"
              disabled={backupBusy !== null}
              onClick={() => void openBackupLocation()}
            >
              <span>{t("settings.backups.location.open")}</span>
            </button>
          </SettingRow>
          <SettingRow
            icon="restore_page"
            title={t("settings.backups.restore.title")}
            description={t("settings.backups.restore.description")}
          >
            <button
              type="button"
              className="select"
              disabled={backupBusy !== null}
              onClick={() => void chooseBackup()}
            >
              <span>{t("settings.backups.restore.action")}</span>
            </button>
          </SettingRow>
        </SettingsSection>

        <SettingsSection title={t("updates.title")}>
          <UpdateSettings />
        </SettingsSection>

        <SettingsSection title={t("settings.about.section")}>
          <SettingRow
            icon="info"
            title={`${t("app.name")} v${version}`}
            description={t("settings.about.appDescription")}
          />
          <SettingRow
            icon="school"
            title={t("settings.about.tutorial.title")}
            description={t("settings.about.tutorial.description")}
          >
            <button type="button" className="select" onClick={replayOnboarding}>
              <span>{t("common.action.replay")}</span>
            </button>
          </SettingRow>
          <SettingRow
            icon="refresh"
            title={t("settings.about.restart.title")}
            description={t("settings.about.restart.description")}
          >
            <button type="button" className="select" onClick={restartApplication}>
              <span>{t("common.action.restart")}</span>
            </button>
          </SettingRow>
          <SettingRow
            icon="restart_alt"
            title={t("settings.about.reset.title")}
            description={t("settings.about.reset.description")}
          >
            <button type="button" className="select" onClick={() => setConfirmingReset(true)}>
              <span>{t("settings.about.reset.action")}</span>
            </button>
          </SettingRow>
        </SettingsSection>
      </div>
      <ConfirmModal
        open={restorePath !== null}
        onClose={() => setRestorePath(null)}
        onCancel={() => {
          void invoke("cancel_backup_restore").catch((reason) => setError(String(reason)));
        }}
        title={t("settings.restoreDialog.title", { name: restorePath ? `“${fileName(restorePath)}”` : t("settings.restoreDialog.backup") })}
        confirmLabel={t("settings.restoreDialog.confirm")}
        onConfirm={() => {
          if (restorePath) void restoreBackup();
        }}
      >
        {t("settings.restoreDialog.body")}
      </ConfirmModal>

      <ConfirmModal
        open={confirmingReset}
        onClose={() => setConfirmingReset(false)}
        title={t("settings.resetDialog.title")}
        confirmLabel={t("settings.resetDialog.confirm")}
        onConfirm={() => void invoke("reset_app").catch((e) => setError(String(e)))}
      >
        {t("settings.resetDialog.body")}
      </ConfirmModal>
    </div>
  );
}
