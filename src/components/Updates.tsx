import { useEffect, useState } from "react";
import { useI18n } from "../i18n";
import { subscribeUpdates, useUpdates } from "../store/updates";
import { restartApplication } from "../hooks/useGlobalShortcuts";

import { getVersion } from "@tauri-apps/api/app";
import { Ms } from "./Icons";

function errorKind(raw: string) {
  const e = raw.toLowerCase();
  if (/signature|minisign/.test(e)) return "signature";
  if (/certificate|tls|ssl|handshake/.test(e)) return "tls";
  if (/timed out|timeout/.test(e)) return "timeout";
  if (/error sending request|dns|connect|network|resolve|unreachable|offline/.test(e)) return "network";
  return "other";
}
function Status({ detailed = false }: { detailed?: boolean }) {
  const { t } = useI18n();
  const info = useUpdates((s) => s.info);
  if (!info) return null;
  if (!info.supported) return <span>{t("updates.unsupported")}</span>;
  if (!info.configured) return <span>{t("updates.unconfigured")}</span>;
  const status = info.status;
  const percent = status.total ? Math.min(100, Math.floor(status.downloaded / status.total * 100)) : null;
  if (status.phase === "error") {
    if (!detailed) return <span>{t("updates.error")}</span>;
    return <span>{t("updates.error")}: {t(`updates.error.${errorKind(status.error ?? "")}`)}
      {status.error && <details className="update-error-details"><summary>{t("updates.error.details")}</summary>
        <code dir="ltr">{status.error}</code></details>}
    </span>;
  }
  return <span>{t(`updates.${status.phase}`, { version: status.version ?? "" })}
    {status.phase === "downloading" && percent !== null ? ` ${percent}%` : ""}
  </span>;
}
export function UpdateBanner() {
  const { t } = useI18n();
  const info = useUpdates((s) => s.info);
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    void subscribeUpdates().then((unlisten) => {
      if (disposed) unlisten(); else stop = unlisten;
    }).catch((error) => useUpdates.setState({ actionError: String(error) }));
    return () => { disposed = true; stop?.(); };
  }, []);
  if (!info?.supported || !["downloading", "installing", "installed"].includes(info.status.phase)) return null;
  return <div className="update-banner" role="status" aria-live="polite">
    <div className="update-banner-body"><Status /><UpdateProgress /></div>
    {info.status.phase === "installed" && <button className="select" onClick={restartApplication}>{t("updates.restart")}</button>}
  </div>;
}
function UpdateProgress() {
  const { t } = useI18n();
  const status = useUpdates((s) => s.info?.status);
  if (!status || !["downloading", "installing"].includes(status.phase)) return null;
  const known = status.phase === "downloading" && status.total !== null && status.total > 0;
  const percent = known ? Math.max(0, Math.min(100, Math.floor(status.downloaded / status.total! * 100))) : undefined;
  const mb = (bytes: number) => `${(bytes / 1048576).toFixed(1)} MB`;
  return <div className="update-progress">
    <div className="update-progress-meta" aria-hidden="true">
      <span>{t(`updates.${status.phase}`, { version: status.version ?? "" })}</span>
      <span dir="ltr">{percent === undefined ? "…" : `${percent}%`}</span>
    </div>
    <div className={"update-progress-track" + (known ? "" : " indeterminate")}
      role="progressbar" aria-label={t(`updates.${status.phase}`, { version: status.version ?? "" })}
      aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent}>
      <div className="update-progress-fill" style={known ? { width: `${percent}%` } : undefined} />
    </div>
    {status.phase === "downloading" && <div className="update-transfer" dir="ltr">
      {mb(status.downloaded)}{known ? ` / ${mb(status.total!)}` : ""}
    </div>}
  </div>;
}

export function UpdateSettings() {
  const { t } = useI18n();
  const { info, toggle, check, pending, actionError } = useUpdates();
  const [version, setVersion] = useState("");
  useEffect(() => { void getVersion().then(setVersion).catch(() => {}); }, []);
  const phase = info?.status.phase ?? "idle";
  const busy = ["checking", "downloading", "installing", "installed"].includes(phase);
  const icon = phase === "installed" || phase === "current" ? "check_circle" : phase === "error" ? "error" : "download";
  return <div className="update-settings">
    <div className="update-overview">
      <span className={"update-emblem phase-" + phase}><Ms name={icon} /></span>
      <div className="update-heading"><strong>{t("updates.title")}</strong>
        <div className="update-status" role="status"><Status detailed /></div>
      </div>
      {version && <span className="update-version" dir="ltr">v{version}</span>}
    </div>
    <UpdateProgress />
    <label className="update-toggle">
      <span><strong>{t("updates.automatic")}</strong><span className="update-description">{t("updates.description")}</span></span>
      <input type="checkbox" role="switch" checked={info?.enabled ?? true} disabled={!info?.supported || pending}
        onChange={(e) => void toggle(e.target.checked)} />
    </label>
    {actionError && <p className="update-action-error" role="alert">{actionError}</p>}
    <div className="update-actions">
      {phase === "installed" ? <button className="select update-primary" onClick={restartApplication}>
        <Ms name="restart_alt" />{t("updates.restart")}</button> :
      <button className="select update-primary" disabled={!info?.supported || !info.configured || pending || busy}
        onClick={() => void check()}><Ms name="refresh" />{t(phase === "checking" ? "updates.checking" : "updates.check")}</button>}
    </div>
  </div>;
}
