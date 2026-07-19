/**
 * Cookie session store (API.md "Authenticated API (v1: cookie session)").
 * Sessions are opaque random ids in a DB table, not JWTs, so logout and
 * expiry are simple row deletes/lookups — no revocation-list complexity.
 */
import { randomUUID } from "node:crypto";
import type { FastifyReply, FastifyRequest } from "fastify";
import type { Db } from "./db.ts";
import { unauthorized } from "./errors.ts";

export const SESSION_COOKIE = "vm_session";
const SESSION_TTL_MS = 30 * 24 * 60 * 60 * 1000; // 30 days

export function createSession(db: Db, userId: number): { id: string; expiresAt: number } {
  const id = randomUUID();
  const now = Date.now();
  const expiresAt = now + SESSION_TTL_MS;
  db.prepare("INSERT INTO sessions (id, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)").run(
    id,
    userId,
    now,
    expiresAt,
  );
  return { id, expiresAt };
}

export function getSessionUserId(db: Db, sessionId: string): number | undefined {
  const row = db.prepare("SELECT user_id, expires_at FROM sessions WHERE id = ?").get(sessionId) as
    | { user_id: number; expires_at: number }
    | undefined;
  if (!row) return undefined;
  if (row.expires_at < Date.now()) {
    db.prepare("DELETE FROM sessions WHERE id = ?").run(sessionId);
    return undefined;
  }
  return row.user_id;
}

export function deleteSession(db: Db, sessionId: string): void {
  db.prepare("DELETE FROM sessions WHERE id = ?").run(sessionId);
}

export function setSessionCookie(reply: FastifyReply, sessionId: string, expiresAt: number): void {
  reply.setCookie(SESSION_COOKIE, sessionId, {
    httpOnly: true,
    sameSite: "lax",
    path: "/",
    signed: true,
    expires: new Date(expiresAt),
  });
}

export function clearSessionCookie(reply: FastifyReply): void {
  reply.clearCookie(SESSION_COOKIE, { path: "/" });
}

/** Returns the authenticated user id, or undefined if not logged in. */
export function currentUserId(request: FastifyRequest, db: Db): number | undefined {
  const cookie = request.cookies[SESSION_COOKIE];
  if (!cookie) return undefined;
  const unsigned = request.unsignCookie(cookie);
  if (!unsigned.valid || !unsigned.value) return undefined;
  return getSessionUserId(db, unsigned.value);
}

/** Same as {@link currentUserId} but throws `unauthorized` when absent. */
export function requireUserId(request: FastifyRequest, db: Db): number {
  const userId = currentUserId(request, db);
  if (userId === undefined) throw unauthorized();
  return userId;
}
