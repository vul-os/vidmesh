/**
 * Registers every API.md route module onto the fastify instance.
 */
import type { FastifyInstance } from "fastify";
import type { AppDeps } from "../app-deps.ts";
import { registerVideoRoutes } from "./videos.ts";
import { registerChannelRoutes } from "./channels.ts";
import { registerSearchRoutes } from "./search.ts";
import { registerRecordRoutes } from "./records.ts";
import { registerPolicyRoutes } from "./policy.ts";
import { registerAuthRoutes } from "./auth.ts";
import { registerMeRoutes } from "./me.ts";
import { registerUploadRoutes } from "./upload.ts";
import { registerSocialRoutes } from "./social.ts";
import { registerComplianceRoutes } from "./compliance.ts";

export function registerApiRoutes(app: FastifyInstance, deps: AppDeps): void {
  registerVideoRoutes(app, deps);
  registerChannelRoutes(app, deps);
  registerSearchRoutes(app, deps);
  registerRecordRoutes(app, deps.db);
  registerPolicyRoutes(app, deps);
  registerAuthRoutes(app, deps);
  registerMeRoutes(app, deps);
  registerUploadRoutes(app, deps);
  registerSocialRoutes(app, deps);
  registerComplianceRoutes(app, deps);
}
