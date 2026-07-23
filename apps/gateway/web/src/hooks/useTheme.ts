import { useCallback, useEffect, useState } from "react";

export type Theme = "light" | "dark";

const STORAGE_KEY = "evermesh:theme";

/** Pure: resolves the initial theme from storage, falling back to the media query. Exported for testing. */
export function resolveInitialTheme(
  storage: Pick<Storage, "getItem">,
  prefersDark: boolean,
): Theme {
  const stored = storage.getItem(STORAGE_KEY);
  if (stored === "dark" || stored === "light") return stored;
  return prefersDark ? "dark" : "light";
}

/** Pure: the other theme. */
export function toggleTheme(theme: Theme): Theme {
  return theme === "dark" ? "light" : "dark";
}

/**
 * Dark-mode toggle, persisted to localStorage with a `prefers-color-scheme`
 * default (index.html applies the same resolution synchronously before
 * paint to avoid a flash; this hook takes over after mount).
 */
export function useTheme(): { theme: Theme; setTheme: (t: Theme) => void; toggle: () => void } {
  const [theme, setThemeState] = useState<Theme>(() => {
    if (typeof window === "undefined") return "light";
    return resolveInitialTheme(window.localStorage, window.matchMedia("(prefers-color-scheme: dark)").matches);
  });

  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    try {
      window.localStorage.setItem(STORAGE_KEY, theme);
    } catch {
      // Storage unavailable (private browsing, quota) — theme just won't persist.
    }
  }, [theme]);

  const setTheme = useCallback((t: Theme) => setThemeState(t), []);
  const toggle = useCallback(() => setThemeState((t) => toggleTheme(t)), []);

  return { theme, setTheme, toggle };
}
