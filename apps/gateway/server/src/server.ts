/**
 * Builds the fastify instance: plugins, error handler, and every route
 * module. Factored out of main.ts so tests can build the exact same app
 * (via fastify.inject) without binding a real port, a live relay, or
 * ffmpeg — main.ts is just this plus `.listen()` and process signals.
 */
import Fastify, { type FastifyInstance } from "fastify";
import fastifyCookie from "@fastify/cookie";
import fastifyMultipart from "@fastify/multipart";
import type { Config } from "./config.ts";
import type { AppDeps } from "./app-deps.ts";
import { registerMediaRoutes } from "./media.ts";
import { registerApiRoutes } from "./api/index.ts";
import { ApiError } from "./errors.ts";

export async function buildServer(config: Config, deps: AppDeps): Promise<FastifyInstance> {
  const app = Fastify({ logger: false, bodyLimit: 10 * 1024 * 1024 });

  await app.register(fastifyCookie, { secret: config.sessionSecret });
  await app.register(fastifyMultipart, {
    limits: { fileSize: config.uploadMaxBytes, files: 1 },
  });

  app.setErrorHandler((err, _request, reply) => {
    if (err instanceof ApiError) {
      reply.code(err.status).send(err.toBody());
      return;
    }
    deps.log(`unhandled error: ${err.message}`);
    reply.code(500).send({ error: { code: "internal", message: "internal server error" } });
  });

  registerMediaRoutes(app, { db: deps.db, config: deps.config, policy: deps.policy, csam: deps.csam });
  registerApiRoutes(app, deps);

  return app;
}
