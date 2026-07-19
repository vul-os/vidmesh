/**
 * `GET /api/policy` (the visible moderation-policy page data, spec
 * 009-gateway.md §1: "a gateway MUST publish a human-readable moderation
 * policy page") and `GET /api/info`. Both read live config — the policy
 * page reflects whatever PolicyEngine currently has loaded, including
 * after a SIGHUP reload.
 */
import type { FastifyInstance } from "fastify";
import type { AppDeps } from "../app-deps.ts";
import type { PolicyPageData, InfoResponse } from "../types.ts";

const GATEWAY_VERSION = "0.1.0";

export function registerPolicyRoutes(app: FastifyInstance, deps: AppDeps): void {
  app.get("/api/policy", async (): Promise<PolicyPageData> => {
    return {
      name: deps.policy.name,
      description: deps.policy.description,
      moderationPolicyHtml: deps.policy.moderationPolicyHtml,
      feeds: deps.policy.subscribedFeeds,
      stats: deps.policy.stats(),
    };
  });

  app.get("/api/info", async (): Promise<InfoResponse> => {
    return {
      gateway: deps.config.gatewayName,
      version: GATEWAY_VERSION,
      relays: deps.config.relays,
      uploadEnabled: deps.config.uploadEnabled,
    };
  });
}
