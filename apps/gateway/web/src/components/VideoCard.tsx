import { Avatar, formatClockTime, TimeAgo } from "@vidmesh/ui";
import { Link } from "react-router-dom";
import type { VideoSummary } from "../lib/api-types.js";

export interface VideoCardProps {
  video: VideoSummary;
}

export function VideoCard({ video }: VideoCardProps): JSX.Element {
  return (
    <Link
      to={`/watch/${encodeURIComponent(video.id)}`}
      className="group block rounded-lg focus-visible:outline focus-visible:outline-[3px] focus-visible:outline-offset-2 focus-visible:outline-accent-600 dark:focus-visible:outline-brand-300"
    >
      <div className="relative aspect-video w-full overflow-hidden rounded-lg bg-slate-200 dark:bg-slate-800">
        {video.thumbnailUrl ? (
          <img src={video.thumbnailUrl} alt="" className="h-full w-full object-cover" loading="lazy" />
        ) : (
          <div className="flex h-full w-full items-center justify-center text-slate-400" aria-hidden="true">
            ▶
          </div>
        )}
        <span className="absolute bottom-1 right-1 rounded bg-black/80 px-1.5 py-0.5 text-xs font-medium text-white">
          {formatClockTime(video.durationMs / 1000)}
        </span>
      </div>
      <div className="mt-2 flex gap-2">
        <Avatar name={video.author.name} src={video.author.avatarUrl} size="sm" />
        <div className="min-w-0">
          <h3 className="truncate text-sm font-semibold decoration-brand-600 decoration-2 underline-offset-2 group-hover:underline dark:decoration-brand-400">{video.title}</h3>
          <p className="truncate text-xs text-slate-600 dark:text-slate-400">{video.author.name}</p>
          {/* API.md's createdAt fields are Unix seconds (unlike the Ms-suffixed
              duration/sponsorship fields), matching the kernel record's native
              createdAt unit — see README.md "API.md gaps". */}
          <TimeAgo unixMs={video.createdAt * 1000} className="text-xs text-slate-500 dark:text-slate-500" />
        </div>
      </div>
    </Link>
  );
}
