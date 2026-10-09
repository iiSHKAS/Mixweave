import { registerBundledLanguagePacks } from "./i18nCore";
import arabic from "./locales/ar.json";
import russian from "./locales/ru.json";
import simplifiedChinese from "./locales/zh-CN.json";

export const BUNDLED_LANGUAGE_PACKS: readonly unknown[] = [arabic, russian, simplifiedChinese];

registerBundledLanguagePacks(BUNDLED_LANGUAGE_PACKS);
