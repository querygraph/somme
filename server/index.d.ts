export type RateLimitState = {
  admin?: boolean;
  limit: number;
  remaining: number;
  windowStart: string;
  cost?: number;
  requestId?: string;
  retryAfter?: string | number;
  warning?: string;
  tier?: string;
};
export type FairUseAlternative = {
  kind?: string;
  label?: string;
  cost?: number;
  command?: string;
  parameters?: unknown;
  [key: string]: unknown;
};
export type FairUseDetails = {
  error?: string;
  code?: string;
  scope?: string;
  cost?: number;
  remaining?: number;
  shortfall?: number;
  retryAt?: string;
  alternatives?: FairUseAlternative[];
  [key: string]: unknown;
};
export function createApiKey(prefix?: string): string;
export function hashApiKey(token: string): string;
export function parseBearer(authorization: unknown, prefix?: string): string | null;
export function adminEmails(value?: string): string[];
export function isAdminEmail(email: unknown, value?: string): boolean;
export function utcWindowStart(now?: Date): string;
export function utcResetEpochSeconds(windowStart?: string): number;
export function normalizeDailyLimit(value: unknown, fallback?: number): number;
export function rateLimitHeaders(state: RateLimitState): Record<string, string>;
export class RateLimitError extends Error {
  constructor(headers: Record<string, string>, fairUse?: FairUseDetails);
  status: 429;
  headers: Record<string, string>;
  fairUse: FairUseDetails;
  fair_use: FairUseDetails;
  body: FairUseDetails & { error: string; code: string };
}
