// Harbor LM report endpoint. POST /v1/reports with
//   Authorization: Bearer <Firebase ID token>
//   {reason, comment?, question?, answer, model_id?, app_version?, platform?}
// Only what the user chose to send in the in-app Report sheet arrives here;
// the app has no other channel to this Worker. Reports are kept
// RETENTION_DAYS days (privacy policy §4).

const REASONS = new Set(["offensive", "harmful", "sexual", "hateful", "illegal", "misinformation", "other"]);
const LIMITS = { comment: 2000, question: 4000, answer: 8000, model_id: 200, app_version: 40, platform: 20 };
const JWKS_URL = "https://www.googleapis.com/service_accounts/v1/jwk/securetoken@system.gserviceaccount.com";

let jwksCache = { keys: null, until: 0 };

function json(status, body) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  });
}

function b64urlBytes(s) {
  const pad = s.length % 4 ? "=".repeat(4 - (s.length % 4)) : "";
  const bin = atob(s.replace(/-/g, "+").replace(/_/g, "/") + pad);
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
}

async function signingKeys() {
  const now = Date.now();
  if (jwksCache.keys && now < jwksCache.until) return jwksCache.keys;
  const res = await fetch(JWKS_URL);
  if (!res.ok) throw new Error("jwks fetch failed");
  const maxAge = Number((res.headers.get("cache-control") || "").match(/max-age=(\d+)/)?.[1] || 3600);
  jwksCache = { keys: (await res.json()).keys, until: now + maxAge * 1000 };
  return jwksCache.keys;
}

// Verifies a Firebase Auth ID token (RS256, issuer/audience = project).
export async function verifyIdToken(token, projectId, keysFn = signingKeys, nowSec = Date.now() / 1000) {
  const parts = token.split(".");
  if (parts.length !== 3) throw new Error("malformed token");
  const header = JSON.parse(new TextDecoder().decode(b64urlBytes(parts[0])));
  const payload = JSON.parse(new TextDecoder().decode(b64urlBytes(parts[1])));
  if (header.alg !== "RS256" || !header.kid) throw new Error("bad header");
  const jwk = (await keysFn()).find((k) => k.kid === header.kid);
  if (!jwk) throw new Error("unknown key");
  const key = await crypto.subtle.importKey("jwk", jwk, { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, false, ["verify"]);
  const ok = await crypto.subtle.verify(
    "RSASSA-PKCS1-v1_5", key, b64urlBytes(parts[2]),
    new TextEncoder().encode(`${parts[0]}.${parts[1]}`));
  if (!ok) throw new Error("bad signature");
  if (payload.aud !== projectId) throw new Error("bad audience");
  if (payload.iss !== `https://securetoken.google.com/${projectId}`) throw new Error("bad issuer");
  if (typeof payload.sub !== "string" || !payload.sub) throw new Error("no subject");
  if (!(payload.exp > nowSec) || !(payload.iat <= nowSec + 300)) throw new Error("expired");
  return payload.sub;
}

function clip(v, n) {
  return typeof v === "string" && v.length ? v.slice(0, n) : null;
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === "/health") return json(200, { ok: true });
    if (url.pathname !== "/v1/reports") return json(404, { error: "not_found" });
    if (request.method !== "POST") return json(405, { error: "method_not_allowed" });

    const auth = request.headers.get("authorization") || "";
    if (!auth.startsWith("Bearer ")) return json(401, { error: "unauthenticated" });
    let uid;
    try {
      uid = await verifyIdToken(auth.slice(7), env.FIREBASE_PROJECT_ID);
    } catch {
      return json(401, { error: "unauthenticated" });
    }

    if (Number(request.headers.get("content-length") || 0) > 32768) return json(413, { error: "too_large" });
    let body;
    try {
      body = await request.json();
    } catch {
      return json(400, { error: "bad_json" });
    }
    if (!REASONS.has(body.reason)) return json(400, { error: "bad_reason" });
    const answer = clip(body.answer, LIMITS.answer);
    if (!answer) return json(400, { error: "answer_required" });

    const hourAgo = new Date(Date.now() - 3600_000).toISOString();
    const { n } = await env.DB.prepare("SELECT COUNT(*) AS n FROM reports WHERE uid = ? AND created_at > ?")
      .bind(uid, hourAgo).first();
    if (n >= Number(env.MAX_REPORTS_PER_HOUR)) return json(429, { error: "rate_limited" });

    const id = crypto.randomUUID();
    await env.DB.prepare(
      `INSERT INTO reports (id, created_at, uid, reason, comment, question, answer, model_id, app_version, platform)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
      .bind(id, new Date().toISOString(), uid, body.reason,
        clip(body.comment, LIMITS.comment), clip(body.question, LIMITS.question), answer,
        clip(body.model_id, LIMITS.model_id), clip(body.app_version, LIMITS.app_version),
        clip(body.platform, LIMITS.platform))
      .run();
    return json(201, { id });
  },

  async scheduled(_event, env) {
    const cutoff = new Date(Date.now() - Number(env.RETENTION_DAYS) * 86400_000).toISOString();
    await env.DB.prepare("DELETE FROM reports WHERE created_at < ?").bind(cutoff).run();
  },
};
