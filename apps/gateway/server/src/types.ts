/**
 * Shared response shapes mirroring apps/gateway/web/API.md exactly. Kept
 * separate from the DB row types (db.ts) so API modules can compose
 * responses without every route re-declaring the contract.
 */

export interface AuthorRef {
  identityId: string;
  name: string;
  avatarUrl?: string;
}

export interface VideoSummary {
  id: string;
  title: string;
  author: AuthorRef;
  thumbnailUrl: string | null;
  durationMs: number;
  createdAt: number;
  channelId?: string;
}

export interface RenditionView {
  height: number;
  hlsUrl: string;
}

export interface CaptionView {
  language: string;
  url: string;
}

export interface SponsorView {
  startMs: number;
  endMs: number;
  label: string;
}

export interface Video extends VideoSummary {
  description: string;
  tags: string[];
  language?: string;
  record: Record<string, unknown>;
  recordCborUrl: string;
  playback: {
    hlsUrl: string | null;
    mp4Url: string | null;
    renditions: RenditionView[];
  };
  captions: CaptionView[];
  license: string;
  payment: [number, string][];
  sponsorship: SponsorView[];
  counts: { comments: number; reactions: Record<string, number> };
}

export interface Comment {
  id: string;
  author: { identityId: string; name: string };
  text: string;
  createdAt: number;
  parent: string | null;
  record: Record<string, unknown>;
}

export interface ClaimView {
  id: string;
  kind: number;
  kindName: string;
  author: string;
  createdAt: number;
  body: Record<string, unknown>;
  targetRecordId: string;
}

export interface ReceiptView {
  id: string;
  author: string;
  createdAt: number;
  amount: number;
  currency: string;
  rail: number;
  payee: string;
  message?: string;
  proof?: string;
}

export interface Channel {
  identityId: string;
  profile: { name: string; about?: string; avatarUrl?: string } | null;
  channels: { id: string; title: string; description?: string; avatarUrl?: string; bannerUrl?: string }[];
  videos: VideoSummary[];
}

export interface PolicyPageData {
  name: string;
  description: string;
  moderationPolicyHtml: string;
  feeds: { feed: string; publisher: string }[];
  stats: { videos: number; deindexed: number; policyLogEntries: number };
}

export interface InfoResponse {
  gateway: string;
  version: string;
  relays: string[];
  uploadEnabled: boolean;
}
