"use client";

import { useEffect, useState } from "react";
import { setWindowMaterialTheme, type ResolvedTheme, type ThemePreference } from "../lib/desktop";

export function useTheme(preference: ThemePreference) {
  const [systemDark, setSystemDark] = useState(() => {
    if (typeof window === "undefined") return true;
    return window.matchMedia("(prefers-color-scheme: dark)").matches;
  });

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const updateFromMedia = () => setSystemDark(media.matches);
    updateFromMedia();
    media.addEventListener("change", updateFromMedia);
    return () => media.removeEventListener("change", updateFromMedia);
  }, []);

  const resolved: ResolvedTheme = preference === "system" ? (systemDark ? "dark" : "light") : preference;

  useEffect(() => {
    document.documentElement.dataset.theme = resolved;
    document.documentElement.style.colorScheme = resolved;
    void setWindowMaterialTheme(preference === "system" ? null : resolved === "dark").catch(() => undefined);
  }, [preference, resolved]);

  return resolved;
}
