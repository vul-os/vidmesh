import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { MusicNoteIcon, PlayIcon } from "@evermesh/ui";
import { useSearchParams } from "react-router-dom";
import { getVideos, search } from "../api.js";
import type { MediaKind } from "../lib/api-types.js";
import { QueryBoundary } from "../components/QueryState.js";
import { VideoGrid } from "../components/VideoGrid.js";

export function Home(): JSX.Element {
  const [params] = useSearchParams();
  const q = params.get("q")?.trim() ?? "";

  return (
    <>
      <Hero />
      {q ? <SearchResults q={q} /> : <LatestMedia />}
    </>
  );
}

/**
 * The gateway app's own front page — the first thing a visitor sees before
 * any video or track has loaded. Built as one instrument strip rather than
 * a centred marketing block: an eyebrow bar in the same "monitor" language
 * as evermesh.org (mono label, signal dot), the tagline as the headline,
 * a one-line value prop naming both media kinds, then a chip row that
 * doubles as an honest capability list. The experimental/DMTAP notice
 * lives in this strip's second column rather than as a separate banner —
 * still first-class, not a dismissible toast, but not competing with the
 * tagline for the reader's first glance either.
 *
 * Spec 009 §7 (uniform UI) requires the DMTAP wording; DECISIONS P17 is
 * why this whole component reads its colours from the token layer instead
 * of hard-coded brand values, so a gateway operator's accent re-skin still
 * applies here.
 */
function Hero(): JSX.Element {
  return (
    <section className="mb-8 overflow-hidden rounded-card border border-line-strong bg-surface shadow-card">
      <div className="flex items-center gap-2 border-b border-line px-4 py-2 sm:px-6">
        <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-signal" aria-hidden="true" />
        <p className="font-mono text-[11px] uppercase tracking-[0.1em] text-faint">On this gateway</p>
      </div>

      <div className="grid gap-7 p-5 sm:p-7 lg:grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)] lg:items-center lg:gap-10">
        <div>
          <h1 className="font-display text-2xl font-extrabold leading-[1.1] tracking-tight text-ink sm:text-[2rem]">
            Many gateways. One substrate.
            <br />
            <span className="text-signal">Media that outlives its platforms.</span>
          </h1>
          <p className="mt-3 max-w-[52ch] text-[15px] text-muted">
            Video and music live side by side here as signed, content-addressed
            records &mdash; verified in your browser before a frame or a beat
            plays, because the math checks out, not because this gateway says
            so.
          </p>

          <ul className="mt-4 flex flex-wrap gap-2" aria-label="What this gateway serves">
            <li className="vm-chip">
              <PlayIcon size={13} /> Video
            </li>
            <li className="vm-chip">
              <MusicNoteIcon size={13} /> Music &amp; playlists
            </li>
            <li className="vm-chip">Client-side verification</li>
            <li className="vm-chip">
              <a
                href="https://github.com/vul-os/evermesh/tree/main/crates/evermesh-node"
                className="hover:text-signal"
              >
                Desktop client ↗
              </a>
            </li>
          </ul>
        </div>

        <aside
          aria-label="Project status"
          className="rounded-control border border-line bg-surface-2 px-4 py-3.5 text-xs leading-relaxed text-muted"
        >
          <p>
            <span className="font-semibold text-live">⚠️ Experimental.</span>{" "}
            Evermesh is early-stage software, not production-ready.
          </p>
          <p className="mt-2">
            It optionally distributes over{" "}
            <a
              href="https://evermesh.org/docs.html#dmtap-convergence"
              className="underline decoration-dotted underline-offset-2 hover:text-ink"
            >
              DMTAP-PUB (§22)
            </a>{" "}
            &mdash; additive, default-off, never a dependency.
          </p>
        </aside>
      </div>
    </section>
  );
}

const MEDIA_TABS: Array<{ value: MediaKind | "all"; label: string }> = [
  { value: "all", label: "All" },
  { value: "video", label: "Video" },
  { value: "audio", label: "Music" },
];

function LatestMedia(): JSX.Element {
  const [params, setParams] = useSearchParams();
  const kindParam = params.get("kind");
  const activeKind: MediaKind | "all" = kindParam === "video" || kindParam === "audio" ? kindParam : "all";

  const query = useInfiniteQuery({
    queryKey: ["videos", activeKind],
    queryFn: ({ pageParam }: { pageParam: string | undefined }) =>
      getVideos({ cursor: pageParam, mediaKind: activeKind === "all" ? undefined : activeKind }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (lastPage) => lastPage.next ?? undefined,
  });

  const items = query.data?.pages.flatMap((p) => p.items) ?? [];

  const setKind = (kind: MediaKind | "all") => {
    const next = new URLSearchParams(params);
    if (kind === "all") next.delete("kind");
    else next.set("kind", kind);
    setParams(next, { replace: true });
  };

  const emptyLabel =
    activeKind === "video"
      ? "This gateway hasn't published any video yet."
      : activeKind === "audio"
        ? "This gateway hasn't published any music yet."
        : "This gateway hasn't published any videos yet.";

  return (
    <div>
      <div className="mb-5 flex flex-wrap items-end justify-between gap-3">
        <div>
          <p className="text-xs font-semibold uppercase tracking-[0.14em] text-signal">On this gateway</p>
          <h2 className="mt-1 text-2xl font-bold tracking-tight text-ink sm:text-3xl">Latest</h2>
        </div>
        <div role="tablist" aria-label="Filter by media kind" className="flex gap-1.5 rounded-control border border-line bg-surface-2 p-1">
          {MEDIA_TABS.map((tab) => (
            <button
              key={tab.value}
              type="button"
              role="tab"
              aria-selected={activeKind === tab.value}
              onClick={() => setKind(tab.value)}
              className={
                "rounded-[0.4rem] px-3 py-1.5 text-sm font-medium transition-colors duration-150 " +
                (activeKind === tab.value ? "bg-surface text-ink shadow-card" : "text-muted hover:text-ink")
              }
            >
              {tab.label}
            </button>
          ))}
        </div>
      </div>
      {query.isLoading ? (
        <p role="status" className="py-6 text-sm text-muted">
          Loading…
        </p>
      ) : query.isError ? (
        <p role="alert" className="py-6 text-sm text-red-700 dark:text-red-300">
          {query.error instanceof Error ? query.error.message : "Could not load the catalogue."}
        </p>
      ) : (
        <>
          <VideoGrid videos={items} emptyLabel={emptyLabel} />
          {query.hasNextPage && (
            <div className="mt-9 flex justify-center">
              <button type="button" onClick={() => void query.fetchNextPage()} disabled={query.isFetchingNextPage} className="vm-btn vm-btn-primary">
                {query.isFetchingNextPage ? "Loading…" : "Load more"}
              </button>
            </div>
          )}
        </>
      )}
    </div>
  );
}

function SearchResults({ q }: { q: string }): JSX.Element {
  const query = useQuery({ queryKey: ["search", q], queryFn: () => search(q) });

  return (
    <div>
      <div className="mb-7">
        <p className="text-xs font-semibold uppercase tracking-[0.14em] text-signal">Search</p>
        <h2 className="mt-1 text-2xl font-bold tracking-tight text-ink sm:text-3xl">
          Results for &ldquo;{q}&rdquo;
        </h2>
      </div>
      <QueryBoundary
        isLoading={query.isLoading}
        isError={query.isError}
        error={query.error}
        data={query.data}
        isEmpty={(d) => d.items.length === 0}
        emptyLabel={`Nothing on this gateway matches "${q}".`}
      >
        {(data) => <VideoGrid videos={data.items} />}
      </QueryBoundary>
    </div>
  );
}
