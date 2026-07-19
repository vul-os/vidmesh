/**
 * Uniform error envelope: `{ error: { code, message } }` (API.md).
 */

export const ERROR_STATUS: Record<string, number> = {
  not_found: 404,
  invalid: 400,
  policy_denied: 404, // disclosure of de-indexed content defaults to not_found-shaped denial
  unauthorized: 401,
  rate_limited: 429,
  upload_failed: 422,
  conflict: 409,
};

export class ApiError extends Error {
  readonly code: string;
  readonly status: number;

  constructor(code: string, message: string, status?: number) {
    super(message);
    this.code = code;
    this.status = status ?? ERROR_STATUS[code] ?? 400;
  }

  toBody(): { error: { code: string; message: string } } {
    return { error: { code: this.code, message: this.message } };
  }
}

export function notFound(message = "not found"): ApiError {
  return new ApiError("not_found", message);
}

export function invalid(message: string): ApiError {
  return new ApiError("invalid", message);
}

export function policyDenied(message = "not found"): ApiError {
  // Per API.md conventions: de-indexed content returns not_found with
  // the policy_denied code where disclosure is lawful.
  return new ApiError("policy_denied", message);
}

export function unauthorized(message = "authentication required"): ApiError {
  return new ApiError("unauthorized", message);
}
