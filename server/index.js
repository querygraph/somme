import { createHash, randomBytes } from "node:crypto";

export function createApiKey(prefix = "somme_sk_") { return `${prefix}${randomBytes(32).toString("base64url")}`; }
export function hashApiKey(token) { return createHash("sha256").update(token).digest("hex"); }
export function parseBearer(authorization, prefix = "somme_sk_") {
  const match = String(authorization || "").trim().match(/^Bearer ([^\s]+)$/i);
  if (!match || !match[1].startsWith(prefix) || match[1].length !== prefix.length + 43) return null;
  return match[1];
}
export function adminEmails(value = process.env.ADMIN_EMAILS || "") { return String(value).split(",").map(email => email.trim().toLowerCase()).filter(Boolean); }
export function isAdminEmail(email, value) { return adminEmails(value).includes(String(email || "").toLowerCase()); }
export function utcWindowStart(now = new Date()) { return now.toISOString().slice(0, 10); }
export function utcResetEpochSeconds(windowStart = utcWindowStart()) { return Math.floor((Date.parse(`${windowStart}T00:00:00.000Z`) + 86_400_000) / 1000); }
export function normalizeDailyLimit(value, fallback = 100) { const parsed = Number(value); return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : fallback; }
export function rateLimitHeaders({ admin = false, limit, remaining, windowStart }) {
  if (admin) return { "x-ratelimit-limit": "unlimited", "x-ratelimit-remaining": "unlimited" };
  return { "x-ratelimit-limit": String(limit), "x-ratelimit-remaining": String(Math.max(0, remaining)), "x-ratelimit-reset": String(utcResetEpochSeconds(windowStart)) };
}
export class RateLimitError extends Error { constructor(headers) { super("API request limit reached"); this.name = "RateLimitError"; this.status = 429; this.headers = headers; } }
