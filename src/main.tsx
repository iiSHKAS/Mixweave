import { useCallback, useState } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ShortcutOsd } from "./components/ShortcutOsd";
import { I18nProvider } from "./i18n";
import type { LocalePreference } from "./i18nCore";
import { reloadLanguagePacks } from "./languagePacks";
import { readLanguagePreference, saveLanguagePreference } from "./store/language";
import { bootTheme } from "./store/theme";
import "./styles/globals.css";

// Apply the saved theme before first paint to avoid a flash of the default.
bootTheme();

// overlay.rs opens a second, dedicated window at "index.html?osd=1" for the
// system-wide shortcut popup (X11 only - see overlay.rs for why). It renders
// only the popup itself, no app chrome, and needs none of the language-pack
// or profile bootstrapping the main window does.
const isOsdWindow = new URLSearchParams(window.location.search).has("osd");

function LanguageRoot() {
  const [preference, setPreference] = useState<LocalePreference>(readLanguagePreference);
  const changePreference = useCallback((next: LocalePreference) => {
    saveLanguagePreference(next);
    setPreference(next);
  }, []);
  return <I18nProvider preference={preference} onPreferenceChange={changePreference}><App /></I18nProvider>;
}

async function start() {
  const root = document.getElementById("root");
  if (!root) return;

  if (isOsdWindow) {
    ReactDOM.createRoot(root).render(<ShortcutOsd />);
    return;
  }

  await reloadLanguagePacks().then((catalog) => {
    if (catalog.warnings.length > 0) console.warn("Mixweave language-pack warnings:", catalog.warnings);
  }).catch((reason: unknown) => {
    console.warn("Mixweave language packs are unavailable:", reason);
  });
  ReactDOM.createRoot(root).render(<LanguageRoot />);
}

void start();
