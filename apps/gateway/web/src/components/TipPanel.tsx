import { CloseIcon, RecordCard } from "@evermesh/ui";
import { useState } from "react";
import type { ReceiptView } from "../lib/api-types.js";

export interface TipPanelProps {
  /** `[railId, paymentPointer][]` from Video.payment (API.md). */
  payment: [number, string][];
  receipts: ReceiptView[];
}

const RAIL_NAMES: Record<number, string> = {
  0: "Lightning",
  1: "On-chain",
  2: "Payment pointer (Interledger/Web Monetization)",
};

function railName(rail: number): string {
  return RAIL_NAMES[rail] ?? `Rail ${rail}`;
}

/**
 * Tip button that opens a display-only modal of this creator's payment
 * pointers plus the public receipt records for this video. There is no
 * payment integration here by design — Evermesh is economically neutral
 * (build plan §1 principle 5); this only surfaces what creators publish
 * and what receipts already exist as records.
 */
export function TipPanel({ payment, receipts }: TipPanelProps): JSX.Element {
  const [open, setOpen] = useState(false);

  return (
    <div>
      <button type="button" onClick={() => setOpen(true)} disabled={payment.length === 0} className="vm-btn vm-btn-accent w-full">
        Tip the creator
      </button>

      {open && (
        <div
          role="dialog"
          aria-modal="true"
          aria-label="Payment pointers"
          className="fixed inset-0 z-20 flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm"
        >
          <div className="vm-fade-up max-h-[80vh] w-full max-w-md overflow-y-auto rounded-card bg-surface p-5 shadow-elevated">
            <div className="mb-3 flex items-center justify-between">
              <h2 className="text-lg font-semibold">Payment pointers</h2>
              <button type="button" onClick={() => setOpen(false)} aria-label="Close" className="vm-icon-btn h-8 w-8 border-transparent">
                <CloseIcon size={16} />
              </button>
            </div>
            <p className="mb-3 text-sm text-muted">
              This gateway does not process payments. Copy a pointer below and send a tip through whatever wallet or client you already use.
            </p>
            <ul className="space-y-2">
              {payment.map(([rail, pointer]) => (
                <li key={`${rail}-${pointer}`} className="rounded-control border border-line bg-surface-2/60 p-2.5 text-sm">
                  <div className="font-medium text-ink">{railName(rail)}</div>
                  <code className="block truncate text-xs text-muted">{pointer}</code>
                </li>
              ))}
            </ul>

            <h3 className="mb-2 mt-4 text-sm font-semibold">Receipts on this gateway</h3>
            {receipts.length === 0 ? (
              <p role="status" className="text-sm text-muted">
                No receipts recorded yet.
              </p>
            ) : (
              <ul className="space-y-2">
                {receipts.map((r) => (
                  <li key={r.id}>
                    <RecordCard
                      author={{ name: `${r.author.slice(0, 10)}…` }}
                      createdAtMs={r.createdAt * 1000}
                      kindLabel="Receipt"
                      body={`${r.amount} ${r.currency}${r.message ? ` — “${r.message}”` : ""}`}
                    />
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
