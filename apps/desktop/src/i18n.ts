import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import enUS from "./locales/en-US.json";
import zhCN from "./locales/zh-CN.json";

const savedLanguage = localStorage.getItem("pga-language");
const detectedLanguage =
  savedLanguage === "zh-CN" || savedLanguage === "en-US"
    ? savedLanguage
    : navigator.language.toLowerCase().startsWith("zh")
      ? "zh-CN"
      : "en-US";

export const i18nReady = i18n.use(initReactI18next).init({
  resources: {
    "en-US": { translation: enUS },
    "zh-CN": { translation: zhCN },
  },
  lng: detectedLanguage,
  fallbackLng: "en-US",
  supportedLngs: ["zh-CN", "en-US"],
  interpolation: { escapeValue: false },
});

export default i18n;
