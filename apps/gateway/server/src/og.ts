/**
 * Server-side share-card fields (API.md "Share cards"): the data behind
 * `og:title`/`og:image`/`og:video` for link unfurling. Pure and
 * fastify-free so both the API route (api/videos.ts) and, eventually, an
 * SSR watch-page route can call it identically.
 */
import type { Config } from "./config.ts";

export interface OgFields {
  title: string;
  description: string;
  image: string | null;
  url: string;
  video: string | null;
}

export interface OgSourceVideo {
  manifestId: string;
  title: string;
  description: string;
  thumbnailBlob: string | null;
}

export function computeOgFields(config: Config, video: OgSourceVideo): OgFields {
  const base = config.publicBaseUrl ?? "";
  return {
    title: video.title,
    description: video.description,
    image: video.thumbnailBlob ? `${base}/media/thumb/${video.thumbnailBlob}` : null,
    url: `${base}/watch/${video.manifestId}`,
    video: null,
  };
}
