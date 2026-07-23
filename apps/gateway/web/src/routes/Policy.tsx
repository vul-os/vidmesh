import { useQuery } from "@tanstack/react-query";
import { getPolicy } from "../api.js";
import { QueryBoundary } from "../components/QueryState.js";

/**
 * The moderation-policy page (spec 009 §1/§7): every gateway MUST
 * publish this, and it MUST NOT be removed from the uniform UI. It also
 * carries the "counts are this gateway's claims" explainer (spec 009
 * §6) so viewers understand that view/comment/reaction counts describe
 * this gateway's index, not the substrate.
 */
export function Policy(): JSX.Element {
  const query = useQuery({ queryKey: ["policy"], queryFn: getPolicy });

  return (
    <QueryBoundary
      isLoading={query.isLoading}
      isError={query.isError}
      error={query.error}
      data={query.data}
      loadingLabel="Loading policy…"
    >
      {(policy) => (
        <div className="max-w-3xl">
          <h1 className="text-xl font-semibold text-ink">{policy.name}</h1>
          <p className="mt-1 text-sm text-muted">{policy.description}</p>

          <dl className="mt-6 grid grid-cols-3 divide-x divide-line rounded-card border border-line bg-surface shadow-card">
            <div className="px-4 py-3.5 text-center sm:text-left sm:px-5">
              <dt className="text-xs font-medium uppercase tracking-wide text-faint">Videos indexed</dt>
              <dd className="mt-1 font-display text-2xl font-bold text-ink">{policy.stats.videos}</dd>
            </div>
            <div className="px-4 py-3.5 text-center sm:text-left sm:px-5">
              <dt className="text-xs font-medium uppercase tracking-wide text-faint">De-indexed</dt>
              <dd className="mt-1 font-display text-2xl font-bold text-ink">{policy.stats.deindexed}</dd>
            </div>
            <div className="px-4 py-3.5 text-center sm:text-left sm:px-5">
              <dt className="text-xs font-medium uppercase tracking-wide text-faint">Policy log entries</dt>
              <dd className="mt-1 font-display text-2xl font-bold text-ink">{policy.stats.policyLogEntries}</dd>
            </div>
          </dl>

          <section className="mt-6 rounded-card border border-line bg-surface-2/50 p-4 text-sm text-ink">
            <p>
              <strong>Counts are this gateway&rsquo;s claims.</strong> View, comment, and reaction counts shown
              throughout this site reflect only what {policy.name} has selected and indexed — not a global count
              across the Evermesh network. Other gateways serving the same content may show different numbers.
            </p>
          </section>

          <section
            className="prose prose-slate mt-6 max-w-none dark:prose-invert"
            // Server-rendered, same-origin moderation policy HTML from this
            // gateway operator (API.md `GET /api/policy`). Not user content.
            dangerouslySetInnerHTML={{ __html: policy.moderationPolicyHtml }}
          />

          <section className="mt-8">
            <h2 className="text-lg font-semibold text-ink">Subscribed takedown feeds</h2>
            {policy.feeds.length === 0 ? (
              <p role="status" className="mt-2 text-sm text-muted">
                This gateway isn&rsquo;t subscribed to any compliance feeds.
              </p>
            ) : (
              <ul className="mt-2 space-y-1.5 text-sm">
                {policy.feeds.map((f) => (
                  <li key={f.feed} className="rounded-control border border-line bg-surface px-3 py-2">
                    <span className="font-medium text-ink">{f.publisher}</span>{" "}
                    <span className="text-muted">—</span> <code className="text-xs text-muted">{f.feed}</code>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>
      )}
    </QueryBoundary>
  );
}
