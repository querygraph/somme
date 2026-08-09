import assert from "node:assert/strict";
import test from "node:test";
import { createApiKey, hashApiKey, isAdminEmail, normalizeDailyLimit, parseBearer, rateLimitHeaders } from "../server/index.js";
test("creates and parses product bearer keys",()=>{const token=createApiKey("bay_sk_");assert.equal(parseBearer(`Bearer ${token}`,"bay_sk_"),token);assert.equal(hashApiKey(token).length,64);assert.equal(parseBearer(`Bearer ${token}`,"other_sk_"),null)});
test("normalizes finite limits",()=>{assert.equal(normalizeDailyLimit("250"),250);assert.equal(normalizeDailyLimit("0"),100)});
test("administrators are explicitly unlimited",()=>{assert.equal(isAdminEmail("ADMIN@example.com","admin@example.com"),true);assert.deepEqual(rateLimitHeaders({admin:true,limit:1,remaining:0,windowStart:"2026-08-08"}),{"x-ratelimit-limit":"unlimited","x-ratelimit-remaining":"unlimited"})});
test("finite users receive standard quota metadata",()=>{assert.deepEqual(rateLimitHeaders({limit:10,remaining:4,windowStart:"2026-08-08"}),{"x-ratelimit-limit":"10","x-ratelimit-remaining":"4","x-ratelimit-reset":"1786233600"})});
