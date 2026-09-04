import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { FluentProvider, webDarkTheme, webLightTheme } from "@fluentui/react-components";
import { App } from "./App";
import i18n, { i18nReady } from "./i18n";
import "./styles.css";

type ThemePreference = "system" | "light" | "dark";

function ThemeHost() {
  const saved = localStorage.getItem("pga-theme");
  const [preference, setPreference] = useState<ThemePreference>(
    saved === "light" || saved === "dark" ? saved : "system",
  );
  const [systemDark, setSystemDark] = useState(
    () => window.matchMedia("(prefers-color-scheme: dark)").matches,
  );
  useEffect(() => {
    const onTheme = (event: Event) =>
      setPreference((event as CustomEvent<ThemePreference>).detail ?? "system");
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onSystemTheme = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    window.addEventListener("pga-theme-changed", onTheme);
    media.addEventListener("change", onSystemTheme);
    return () => {
      window.removeEventListener("pga-theme-changed", onTheme);
      media.removeEventListener("change", onSystemTheme);
    };
  }, []);
  const theme =
    preference === "dark"
      ? webDarkTheme
      : preference === "light"
        ? webLightTheme
        : systemDark
          ? webDarkTheme
          : webLightTheme;
  return (
    <FluentProvider theme={theme}>
      <App />
    </FluentProvider>
  );
}

async function bootstrap() {
  if (import.meta.env.DEV && new URLSearchParams(window.location.search).get("demo") === "docs") {
    const { installDocsDemoBridge } = await import("./docsDemo");
    installDocsDemoBridge();
  }
  await i18nReady;

  const language = i18n.resolvedLanguage ?? "en-US";
  document.documentElement.lang = language;
  document.title = i18n.t("appName");

  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <ThemeHost />
    </StrictMode>,
  );
}

void bootstrap();
