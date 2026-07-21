import { Link } from "react-router-dom";

export function NotFound(): JSX.Element {
  return (
    <div className="flex flex-col items-center py-20 text-center">
      {/* Three unlinked mesh nodes — the record this URL would point to was
          never woven into this gateway's edge. */}
      <svg viewBox="0 0 120 60" aria-hidden="true" className="mb-5 h-10 w-20 text-line-strong">
        <g fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeDasharray="1 7">
          <path d="M20,15 L60,45 L100,15" />
        </g>
        <g fill="currentColor">
          <circle cx="20" cy="15" r="4" />
          <circle cx="100" cy="15" r="4" />
        </g>
        <circle cx="60" cy="45" r="5" className="fill-none stroke-live" strokeWidth={2} />
      </svg>
      <h1 className="text-xl font-semibold text-ink">Page not found</h1>
      <p className="mt-2 text-sm text-muted">There&rsquo;s nothing at this address on this gateway.</p>
      <Link to="/" className="vm-btn vm-btn-secondary mt-5">
        Back to the latest videos
      </Link>
    </div>
  );
}
