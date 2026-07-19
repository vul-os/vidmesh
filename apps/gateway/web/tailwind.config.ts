import type { Config } from "tailwindcss";

/**
 * The reference UI's palette, wired entirely through CSS custom properties
 * declared in src/styles/index.css (mirrored from assets/tokens.css, the
 * project-wide source of truth documented in assets/README.md).
 *
 * `brand` is signal lime — live, verified, actionable. `accent` is mesh
 * blue — identity, verification, the substrate. Crucially `slate` and
 * `red` are remapped too: components across this app and @vidmesh/ui reach
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
  50: `var(--vm-${name}-50)`,
  100: `var(--vm-${name}-100)`,
  200: `var(--vm-${name}-200)`,
  300: `var(--vm-${name}-300)`,
  400: `var(--vm-${name}-400)`,
  500: `var(--vm-${name}-500)`,
  600: `var(--vm-${name}-600)`,
  700: `var(--vm-${name}-700)`,
  800: `var(--vm-${name}-800)`,
  900: `var(--vm-${name}-900)`,
  950: `var(--vm-${name}-950)`,
});

export default {
  darkMode: "class",
  content: [
    "./index.html",
    "./src/**/*.{ts,tsx}",
    // @vidmesh/ui ships .tsx sources with no build step; Tailwind's JIT
    // scanner must see them directly or their classes get purged.
    "../../../packages/ui/src/**/*.{ts,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        brand: { ...ramp("brand"), DEFAULT: "var(--vm-brand-600)" },
        accent: { ...ramp("accent"), DEFAULT: "var(--vm-accent-600)" },
        // neutrals: one paper-to-carbon ramp, used with `dark:` variants
        slate: ramp("neutral"),
        // errors and record state
        red: ramp("live"),
        // semantic shorthands for new code
        surface: {
          DEFAULT: "var(--vm-surface)",
          2: "var(--vm-surface-2)",
          base: "var(--vm-bg)",
        },
        line: "var(--vm-border)",
        ink: "var(--vm-fg)",
        muted: "var(--vm-muted)",
        signal: "var(--vm-signal)",
        verified: "var(--vm-verified)",
        live: "var(--vm-live)",
      },
      fontFamily: {
        display: "var(--vm-font-display)",
        sans: "var(--vm-font-sans)",
        mono: "var(--vm-font-mono)",
      },
    },
  },
  plugins: [],
} satisfies Config;
