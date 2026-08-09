export type RateLimitState = { admin?: boolean; limit: number; remaining: number; windowStart: string };
export function createApiKey(prefix?: string): string;
export function hashApiKey(token: string): string;
export function parseBearer(authorization: unknown, prefix?: string): string | null;
export function adminEmails(value?: string): string[];
export function isAdminEmail(email: unknown, value?: string): boolean;
export function utcWindowStart(now?: Date): string;
export function utcResetEpochSeconds(windowStart?: string): number;
export function normalizeDailyLimit(value: unknown, fallback?: number): number;
export function rateLimitHeaders(state: RateLimitState): Record<string, string>;
export class RateLimitError extends Error { status: number; headers: Record<string, string>; }
