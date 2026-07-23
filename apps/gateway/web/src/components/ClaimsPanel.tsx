import { RecordCard } from "@evermesh/ui";
import type { ClaimView } from "../lib/api-types.js";

export interface ClaimsPanelProps {
  claims: ClaimView[];
}

/**
 * Presents provenance claims (spec 005) honestly: these are signed
 * assertions someone made, with an author and a chain of custody — not
 * an adjudicated fact. The UI must never say "verified truth"; it says
 * "assertions with provenance" (build plan §10, spec 005).
 */
export function ClaimsPanel({ claims }: ClaimsPanelProps): JSX.Element {
  return (
    <section aria-labelledby="claims-heading">
      <h2 id="claims-heading" className="mb-1 text-base font-semibold">
        Provenance claims
      </h2>
      <p className="mb-3 text-sm text-muted">
        These are signed assertions with provenance — who said it, and when — not adjudicated or verified truth.
      </p>
      {claims.length === 0 ? (
        <p role="status" className="py-4 text-sm text-muted">
          No claims have been made about this video.
        </p>
      ) : (
        <ul className="space-y-2">
          {claims.map((claim) => (
            <li key={claim.id}>
              <RecordCard
                author={{ name: shortenAuthor(claim.author) }}
                createdAtMs={claim.createdAt * 1000}
                kindLabel={claim.kindName}
                body={<ClaimBody claim={claim} />}
              />
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function ClaimBody({ claim }: { claim: ClaimView }): JSX.Element {
  const entries = Object.entries(claim.body);
  if (entries.length === 0) return <span className="text-muted">No further detail.</span>;
  return (
    <dl className="grid grid-cols-[max-content_1fr] gap-x-2 gap-y-0.5 text-xs">
      {entries.map(([key, value]) => (
        <div key={key} className="contents">
          <dt className="font-medium text-muted">{key}</dt>
          <dd className="break-words font-mono text-ink">{formatValue(value)}</dd>
        </div>
      ))}
    </dl>
  );
}

function formatValue(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return JSON.stringify(value);
}

function shortenAuthor(identityId: string): string {
  return `${identityId.slice(0, 10)}…`;
}
