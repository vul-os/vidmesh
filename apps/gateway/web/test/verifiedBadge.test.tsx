import { VerifiedBadge } from "@evermesh/ui";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { KernelLike } from "../src/lib/verify.js";
import { verifyRecordById } from "../src/lib/verify.js";

const bytes = new Uint8Array([1, 2, 3]).buffer;
const fetchCbor = async () => bytes;

describe("verifyRecordById (the badge's underlying state machine, mocked kernel)", () => {
  it("verifies when the kernel accepts the record and the derived id matches the request", async () => {
    const kernel: KernelLike = {
      verifyRecord: vi.fn().mockResolvedValue(undefined),
      deriveId: vi.fn().mockResolvedValue("abc123abc123extra"),
    };

    const result = await verifyRecordById("abc123abc123extra", fetchCbor, kernel);

    expect(result).toEqual({ status: "verified", shortId: "abc123abc123" });
  });

  it("fails when the derived id does not match the requested id", async () => {
    const kernel: KernelLike = {
      verifyRecord: vi.fn().mockResolvedValue(undefined),
      deriveId: vi.fn().mockResolvedValue("some-other-id"),
    };

    const result = await verifyRecordById("abc123", fetchCbor, kernel);

    expect(result.status).toBe("failed");
  });

  it("fails when the kernel rejects the signature/envelope check", async () => {
    const kernel: KernelLike = {
      verifyRecord: vi.fn().mockRejectedValue(new Error("bad signature")),
      deriveId: vi.fn(),
    };

    const result = await verifyRecordById("abc123", fetchCbor, kernel);

    expect(result.status).toBe("failed");
    if (result.status === "failed") expect(result.reason).toContain("bad signature");
  });

  it("fails when the record bytes can't be fetched", async () => {
    const kernel: KernelLike = { verifyRecord: vi.fn(), deriveId: vi.fn() };

    const result = await verifyRecordById(
      "abc123",
      async () => {
        throw new Error("network down");
      },
      kernel,
    );

    expect(result.status).toBe("failed");
  });
});

describe("VerifiedBadge", () => {
  it("renders the verifying state with icon + text", () => {
    render(<VerifiedBadge state="verifying" />);
    expect(screen.getByRole("button", { name: /verifying/i })).toBeInTheDocument();
  });

  it("renders the verified state and reveals what was checked on click", () => {
    render(<VerifiedBadge state="verified" shortId="abc123abc123" />);
    // The ✓ icon is aria-hidden (decorative), so the accessible name is
    // just the label — a screen reader hears "Verified", not "check Verified".
    const button = screen.getByRole("button", { name: /^verified$/i });
    expect(button).toHaveAttribute("aria-expanded", "false");

    fireEvent.click(button);

    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText(/manifest signature verified in your browser/i)).toBeInTheDocument();
    expect(screen.getByText(/abc123abc123/)).toBeInTheDocument();
  });

  it("renders the failed state distinctly, never relying on color alone", () => {
    render(<VerifiedBadge state="failed" failureReason="signature mismatch" />);
    const button = screen.getByRole("button", { name: /verification failed/i });
    // Icon (✕) + text both present — not color-only signaling.
    expect(button).toHaveTextContent("✕");
    expect(button).toHaveTextContent("Verification failed");

    fireEvent.click(button);
    expect(screen.getByText(/signature mismatch/i)).toBeInTheDocument();
  });
});
