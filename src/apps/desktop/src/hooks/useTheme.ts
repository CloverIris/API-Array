"use client";

import { useEffect, useState } from "react";
import { setWindowMaterialTheme, type ResolvedTheme, type ThemePreference } from "../lib/desktop";

export function useTheme(preference: ThemePreference) {
  const [systemDark, setSystemDark] = useState(false);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const update = () => setSystemDark(media.matches);
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);

  const resolved: ResolvedTheme = preference === "system" ? (systemDark ? "dark" : "light") : preference;

  useEffect(() => {
    document.documentElement.dataset.theme = resolved;
    document.documentElement.style.colorScheme = resolved;
    void setWindowMaterialTheme(resolved === "dark").catch(() => undefined);
  }, [resolved]);

  return resolved;
}
