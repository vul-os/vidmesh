import { MoonIcon, SearchIcon, SunIcon } from "@evermesh/ui";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { Link, Outlet, useLocation, useNavigate } from "react-router-dom";
import { useMe } from "../hooks/useMe.js";
import { useTheme } from "../hooks/useTheme.js";

/**
 * The header/nav/footer shell every page renders inside. Owns the parts
 * of the uniform UI that must not vary by gateway product decisions:
 * the skip link, the dark-mode toggle, and the "powered by evermesh" +
 * moderation-policy footer link (spec 009 §7).
 */
export function Layout(): JSX.Element {
  const { theme, toggle } = useTheme();
  const { data: me } = useMe();
  const location = useLocation();
  const navigate = useNavigate();
  const mainRef = useRef<HTMLDivElement>(null);
  const [q, setQ] = useState("");

  // Focus management on route change: move focus to the main landmark
  // so screen-reader/keyboard users land on new content, not stuck on
  // whatever link they clicked in the old page.
  useEffect(() => {
    mainRef.current?.focus();
  }, [location.pathname]);

  const onSearch = (e: FormEvent) => {
    e.preventDefault();
    const trimmed = q.trim();
    navigate(trimmed ? `/?q=${encodeURIComponent(trimmed)}` : "/");
  };

  return (
    <div className="flex min-h-screen flex-col bg-surface-base text-ink">
      <a href="#main" className="skip-link">
        Skip to content
      </a>

      <header className="sticky top-0 z-30 border-b border-line bg-surface-base/85 backdrop-blur supports-[backdrop-filter]:bg-surface-base/70">
        <div className="mx-auto flex max-w-7xl flex-wrap items-center gap-4 px-4 py-3">
          {/* The lockup, drawn inline: the mark inherits the signal colour
              from the token layer and the wordmark is the display face, so
              the header is branded without loading an image. */}
          <Link to="/" className="group flex shrink-0 items-center gap-2" aria-label="Home">
            <svg viewBox="0 0 256 256" aria-hidden="true" className="h-6 w-6 shrink-0 transition-transform duration-200 group-hover:scale-110">
              <path
                className="fill-ink"
                d="M40,187 C40,159 82,143 128,143 C174,143 216,159 216,187 C216,213 174,227 128,227 C82,227 40,213 40,187 Z"
              />
              <path
                className="fill-ink"
                d="M66,124 C66,101 98,86 132,86 C166,86 194,101 194,124 C194,145 166,159 132,159 C98,159 66,145 66,124 Z"
              />
              <path
                className="fill-brand-700 dark:fill-brand-400"
                d="M90,68 C90,49 110,37 132,37 C154,37 172,49 172,68 C172,85 154,97 132,97 C110,97 90,85 90,68 Z"
              />
            </svg>
            <span className="font-display text-lg font-extrabold tracking-tight">evermesh</span>
          </Link>

          <form role="search" onSubmit={onSearch} className="flex min-w-[12rem] flex-1 items-center">
            <label htmlFor="site-search" className="sr-only">
              Search videos
            </label>
            <div className="relative w-full">
              <SearchIcon size={15} className="pointer-events-none absolute left-3.5 top-1/2 -translate-y-1/2 text-faint" />
              <input
                id="site-search"
                type="search"
                value={q}
                onChange={(e) => setQ(e.target.value)}
                placeholder="Search this gateway…"
                className="vm-field pl-10"
              />
            </div>
          </form>

          <nav aria-label="Primary">
            <ul className="flex items-center gap-5 text-sm font-medium text-muted [&_a]:relative [&_a]:py-1 [&_a]:transition-colors [&_a:hover]:text-ink">
              <li>
                <Link to="/upload">Upload</Link>
              </li>
              <li>
                <Link to="/policy">Policy</Link>
              </li>
              <li>
                <Link to={me ? "/me" : "/auth"} className="text-ink">
                  {me ? me.handle : "Sign in"}
                </Link>
              </li>
            </ul>
          </nav>

          <button type="button" onClick={toggle} aria-label={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"} className="vm-icon-btn">
            {theme === "dark" ? <SunIcon size={17} /> : <MoonIcon size={17} />}
          </button>
        </div>
      </header>

      <div id="main" ref={mainRef} tabIndex={-1} className="mx-auto w-full max-w-7xl flex-1 px-4 py-8 outline-none">
        <Outlet />
      </div>

      <footer className="border-t border-line bg-surface py-6 text-sm text-muted">
        <div className="mx-auto flex max-w-7xl flex-wrap items-center justify-between gap-2 px-4">
          <p>
            Powered by{" "}
            <a href="https://evermesh.org" className="font-medium text-signal hover:underline">
              evermesh
            </a>
            {" "}— many gateways, one substrate.
          </p>
          <Link to="/policy" className="hover:text-ink hover:underline">
            What this gateway serves
          </Link>
        </div>
      </footer>
    </div>
  );
}
