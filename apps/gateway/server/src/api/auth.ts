/**
 * `POST /api/auth/register`, `/login`, `/logout` (API.md). Registration
 * creates a custodied identity (custody.ts) and publishes its genesis
 * record; both endpoints set/clear the session cookie.
 */
import type { FastifyInstance } from "fastify";
import { z } from "zod";
import type { AppDeps } from "../app-deps.ts";
import { invalid, unauthorized } from "../errors.ts";
import { createSession, deleteSession, setSessionCookie, clearSessionCookie, SESSION_COOKIE } from "../session.ts";

const CredentialsSchema = z.object({
  handle: z
    .string()
    .min(3)
    .max(32)
    .regex(/^[a-z0-9_-]+$/, "handle must be lowercase letters, digits, - or _"),
  password: z.string().min(8).max(256),
});

export function registerAuthRoutes(app: FastifyInstance, deps: AppDeps): void {
  app.post("/api/auth/register", async (request, reply) => {
    const parsed = CredentialsSchema.safeParse(request.body);
    if (!parsed.success) throw invalid(parsed.error.message);
    const { handle, password } = parsed.data;

    const { userId } = await deps.custody.register(handle, password);
    const session = createSession(deps.db, userId);
    setSessionCookie(reply, session.id, session.expiresAt);
    reply.code(201);
    return { handle, userId };
  });

  app.post("/api/auth/login", async (request, reply) => {
    const parsed = CredentialsSchema.pick({ handle: true, password: true }).safeParse(request.body);
    if (!parsed.success) throw invalid(parsed.error.message);
    const { handle, password } = parsed.data;

    const user = deps.custody.getUserByHandle(handle);
    if (!user) throw unauthorized("invalid handle or password");
    const ok = await deps.custody.verifyPassword(user.id, password);
    if (!ok) throw unauthorized("invalid handle or password");

    const session = createSession(deps.db, user.id);
    setSessionCookie(reply, session.id, session.expiresAt);
    return { handle: user.handle };
  });

  app.post("/api/auth/logout", async (request, reply) => {
    const cookie = request.cookies[SESSION_COOKIE];
    if (cookie) {
      const unsigned = request.unsignCookie(cookie);
      if (unsigned.valid && unsigned.value) deleteSession(deps.db, unsigned.value);
    }
    clearSessionCookie(reply);
    return { ok: true };
  });
}
