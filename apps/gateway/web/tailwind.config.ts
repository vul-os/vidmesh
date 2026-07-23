import type { Config } from "tailwindcss";

/**
 * The reference UI's palette, wired entirely through CSS custom properties
 * declared in src/styles/index.css (mirrored from assets/tokens.css, the
 * project-wide source of truth documented in assets/README.md).
 *
 * `brand` is signal lime — live, verified, actionable. `accent` is mesh
 * blue — identity, verification, the substrate. Crucially `slate` and
 * `red` are remapped too: components across this app and @evermesh/ui reach
 * for stock `slate-*` neutrals and `red-*` errors, so overriding them here
 * means the whole interface re-skins from the token layer with no
 * component edits — which is exactly the constraint spec 009 §7 puts on a
 * gateway operator (accents only, never the interface).
 *
 * `darkMode: "class"` because the theme toggle (src/hooks/useTheme.ts)
 * sets `dark` on `<html>` explicitly (persisted, defaulting to
 * `prefers-color-scheme`) rather than relying purely on the media query.
 */
const ramp = (name: string) => ({
  50: `var(--bo-${name}-50)`,
  100: `var(--bo-${name}-100)`,
  200: `var(--bo-${name}-200)`,
  300: `var(--bo-${name}-300)`,
  400: `var(--bo-${name}-400)`,
  500: `var(--bo-${name}-500)`,
  600: `var(--bo-${name}-600)`,
  700: `var(--bo-${name}-700)`,
  800: `var(--bo-${name}-800)`,
  900: `var(--bo-${name}-900)`,
  950: `var(--bo-${name}-950)`,
});

export default {
  darkMode: "class",
  content: [
    "./index.html",
    "./src/**/*.{ts,tsx}",
    // @evermesh/ui ships .tsx sources with no build step; Tailwind's JIT
    // scanner must see them directly or their classes get purged.
    "../../../packages/ui/src/**/*.{ts,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        brand: { ...ramp("brand"), DEFAULT: "var(--bo-brand-600)" },
        accent: { ...ramp("accent"), DEFAULT: "var(--bo-accent-600)" },
        // neutrals: one paper-to-carbon ramp, used with `dark:` variants
        slate: ramp("neutral"),
        // errors and record state
        red: ramp("live"),
        // semantic shorthands for new code
        surface: {
          DEFAULT: "var(--bo-surface)",
          2: "var(--bo-surface-2)",
          base: "var(--bo-bg)",
        },
        line: "var(--bo-border)",
        "line-strong": "var(--bo-border-strong)",
        ink: "var(--bo-fg)",
        muted: "var(--bo-muted)",
        faint: "var(--bo-faint)",
        signal: "var(--bo-signal)",
        verified: "var(--bo-verified)",
        live: "var(--bo-live)",
      },
      fontFamily: {
        display: "var(--bo-font-display)",
        sans: "var(--bo-font-sans)",
        mono: "var(--bo-font-mono)",
      },
      borderRadius: {
        card: "var(--bo-radius)",
        control: "var(--bo-radius-sm)",
      },
      boxShadow: {
        card: "var(--bo-shadow)",
        elevated: "var(--bo-shadow-lg)",
      },
      transitionTimingFunction: {
        vm: "var(--bo-ease)",
      },
    },
  },
  plugins: [],
} satisfies Config;
