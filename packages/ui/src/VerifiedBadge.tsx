import { useId, useState } from "react";
import { cn } from "./cn.js";

/**
 * Three-state client-side verification badge. This is the trust anchor
 * of the uniform UI (spec 009 §7) — gateways MUST NOT remove it. It
 * never signals state by color alone: every state pairs an icon with
 * text, and the detail popover always explains WHAT was checked in the
 * viewer's own browser (not "trust us").
 */
export type VerifiedState = "verifying" | "verified" | "failed";

export interface VerifiedBadgeProps {
  state: VerifiedState;
  /** Short hex prefix of the record id, shown once verified/failed. */
  shortId?: string;
  /** Extra detail for the failed state (e.g. "signature mismatch"). */
  failureReason?: string;
  className?: string;
}

/*
 * Colour here is deliberately *mesh blue*, not the signal lime used for
 * primary actions: "verified" is a statement about the substrate, and it
 * must not read as a button or as promotional emphasis. Failure takes the
 * live red, the only other colour this interface is allowed to shout in.
 * Both states carry an icon and a word, so the badge is still legible with
 * no colour perception at all (WCAG 1.4.1).
 */
const STATE_COPY: Record<VerifiedState, { label: string; icon: string; ring: string }> = {
  verifying: {
    label: "Verifying…",
    icon: "◐",
    ring: "border-slate-300 bg-slate-100 text-slate-700 dark:border-slate-600 dark:bg-slate-800 dark:text-slate-200",
  },
  verified: {
    label: "Verified",
    icon: "✓",
    ring: "border-accent-600 bg-accent-50 text-accent-900 dark:border-accent-300 dark:bg-accent-950 dark:text-accent-100",
  },
  failed: {
    label: "Verification failed",
    icon: "✕",
    ring: "border-red-600 bg-red-50 text-red-900 dark:border-red-300 dark:bg-red-950 dark:text-red-100",
  },
};

export function VerifiedBadge({ state, shortId, failureReason, className }: VerifiedBadgeProps): JSX.Element {
  const [open, setOpen] = useState(false);
  const popoverId = useId();
  const copy = STATE_COPY[state];

  return (
    <div className={cn("relative inline-block", className)}>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={popoverId}
        onClick={() => setOpen((v) => !v)}
        className={cn(
          "inline-flex items-center gap-1.5 rounded-full border px-3 py-1 text-sm font-medium",
          "transition-colors motion-reduce:transition-none",
          "focus-visible:outline focus-visible:outline-[3px] focus-visible:outline-offset-2 focus-visible:outline-accent-600 dark:focus-visible:outline-brand-300",
          copy.ring,
        )}
      >
        <span aria-hidden="true" className={state === "verifying" ? "animate-pulse motion-reduce:animate-none" : undefined}>
          {copy.icon}
        </span>
        <span>{copy.label}</span>
      </button>

      {open && (
        <div
          id={popoverId}
          role="dialog"
          aria-label="Verification detail"
          className={cn(
            "absolute left-0 z-10 mt-2 w-72 rounded-lg border p-3 text-sm shadow-lg",
            "border-slate-200 bg-white text-slate-800 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-200",
          )}
        >
          {state === "verifying" && (
            <p>Fetching the record's signed bytes and checking the manifest signature in your browser. No server is trusted for this result.</p>
          )}
          {state === "verified" && (
            <>
              <p>Manifest signature verified in your browser: the Ed25519 signature matches the record's public key, and the record id was re-derived from its bytes and matched.</p>
              {shortId && (
                <p className="mt-2 font-mono text-xs text-slate-500 dark:text-slate-400">
                  record <span title="Full record id available via the record's JSON view.">{shortId}…</span>
                </p>
              )}
            </>
          )}
          {state === "failed" && (
            <>
              <p>The signature or record id could not be verified in your browser. This content may be corrupted, mis-served, or tampered with in transit — it is not necessarily false, but its authenticity is unconfirmed.</p>
              {failureReason && <p className="mt-2 text-xs">{failureReason}</p>}
            </>
          )}
        </div>
      )}
    </div>
  );
}
