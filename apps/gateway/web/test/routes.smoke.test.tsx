import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";

/**
 * Route smoke tests: every page renders (no throw) against mocked
 * queries, and shows the right loading/empty/error copy for the
 * "signed out, empty gateway" case — the state a brand-new gateway
 * deploy is actually in.
 */
class MockApiError extends Error {
  constructor(
    readonly code: string,
    message: string,
    readonly status: number,
  ) {
    super(message);
  }
}

vi.mock("../src/api.js", () => ({
  ApiError: MockApiError,
  getVideos: vi.fn().mockResolvedValue({ items: [], next: null }),
  search: vi.fn().mockResolvedValue({ items: [] }),
  getVideo: vi.fn().mockRejectedValue(new MockApiError("not_found", "not found", 404)),
  getVideoComments: vi.fn().mockResolvedValue({ items: [] }),
  getVideoClaims: vi.fn().mockResolvedValue({ items: [] }),
  getVideoReceipts: vi.fn().mockResolvedValue({ items: [] }),
  getChannel: vi.fn().mockRejectedValue(new MockApiError("not_found", "not found", 404)),
  getChannelVideos: vi.fn().mockResolvedValue({ items: [], next: null }),
  getRecord: vi.fn(),
  getRecordCbor: vi.fn().mockResolvedValue(new ArrayBuffer(0)),
  getPolicy: vi.fn().mockResolvedValue({
    name: "Test Gateway",
    description: "A test gateway.",
    moderationPolicyHtml: "<p>We serve what we choose to serve.</p>",
    feeds: [],
    stats: { videos: 0, deindexed: 0, policyLogEntries: 0 },
  }),
  getInfo: vi.fn(),
  register: vi.fn(),
  login: vi.fn(),
  logout: vi.fn(),
  getMe: vi.fn().mockRejectedValue(new MockApiError("unauthorized", "sign in required", 401)),
  exportIdentity: vi.fn(),
  updateProfile: vi.fn(),
  upload: vi.fn(),
  getUploadStatus: vi.fn(),
  postComment: vi.fn(),
  postReaction: vi.fn(),
  follow: vi.fn(),
  unfollow: vi.fn(),
  postComplianceNotice: vi.fn(),
  postComplianceCounter: vi.fn(),
  getComplianceNotice: vi.fn(),
}));

vi.mock("@evermesh/kernel", () => ({
  verifyRecord: vi.fn().mockResolvedValue(undefined),
  deriveId: vi.fn().mockResolvedValue("id"),
}));

const { Auth } = await import("../src/routes/Auth.js");
const { Channel } = await import("../src/routes/Channel.js");
const { Home } = await import("../src/routes/Home.js");
const { NotFound } = await import("../src/routes/NotFound.js");
const { Policy } = await import("../src/routes/Policy.js");
const { Upload } = await import("../src/routes/Upload.js");
const { Watch } = await import("../src/routes/Watch.js");

function renderRoute(ui: ReactElement, initialEntry: string, path: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[initialEntry]}>
        <Routes>
          <Route path={path} element={ui} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("route smoke tests", () => {
  it("Home renders the empty-gateway state", async () => {
    renderRoute(<Home />, "/", "/");
    expect(await screen.findByText(/hasn't published any videos yet/i)).toBeInTheDocument();
  });

  it("Policy renders the moderation policy and the per-gateway counts explainer", async () => {
    renderRoute(<Policy />, "/policy", "/policy");
    expect(await screen.findByText("Test Gateway")).toBeInTheDocument();
    expect(screen.getByText(/counts are this gateway/i)).toBeInTheDocument();
    expect(screen.getByText(/we serve what we choose to serve/i)).toBeInTheDocument();
  });

  it("Auth renders sign-in / create-account tabs", () => {
    renderRoute(<Auth />, "/auth", "/auth");
    expect(screen.getByRole("tab", { name: /sign in/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /create account/i })).toBeInTheDocument();
  });

  it("NotFound renders a way back home", () => {
    renderRoute(<NotFound />, "/nope", "*");
    expect(screen.getByText(/page not found/i)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /back to the latest videos/i })).toBeInTheDocument();
  });

  it("Upload gates behind sign-in when signed out", async () => {
    renderRoute(<Upload />, "/upload", "/upload");
    expect(await screen.findByRole("link", { name: /sign in/i })).toBeInTheDocument();
  });

  it("Watch shows a friendly error, not a crash, for a video this gateway doesn't have", async () => {
    renderRoute(<Watch />, "/watch/abc123", "/watch/:id");
    expect(await screen.findByRole("alert")).toHaveTextContent(/not found/i);
  });

  it("Channel shows a friendly error, not a crash, for an unknown identity", async () => {
    renderRoute(<Channel />, "/channel/abc123", "/channel/:identityId");
    expect(await screen.findByRole("alert")).toHaveTextContent(/not found/i);
  });
});
