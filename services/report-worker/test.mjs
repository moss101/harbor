// node services/report-worker/test.mjs — token verification against a
// locally generated key (no network).
import worker, { verifyIdToken } from "./src/index.js";
import assert from "node:assert/strict";

const { publicKey, privateKey } = await crypto.subtle.generateKey(
  { name: "RSASSA-PKCS1-v1_5", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" },
  true, ["sign", "verify"]);
const jwk = { ...(await crypto.subtle.exportKey("jwk", publicKey)), kid: "k1" };
const b64u = (b) => Buffer.from(b).toString("base64url");
async function token(payload, kid = "k1") {
  const h = b64u(JSON.stringify({ alg: "RS256", kid }));
  const p = b64u(JSON.stringify(payload));
  const sig = await crypto.subtle.sign("RSASSA-PKCS1-v1_5", privateKey, new TextEncoder().encode(`${h}.${p}`));
  return `${h}.${p}.${b64u(sig)}`;
}
const now = 1_800_000_000;
const good = { aud: "harbor-lm", iss: "https://securetoken.google.com/harbor-lm", sub: "u1", iat: now - 10, exp: now + 3600 };
const keys = async () => [jwk];

assert.equal(await verifyIdToken(await token(good), "harbor-lm", keys, now), "u1");
for (const bad of [{ ...good, aud: "other" }, { ...good, iss: "https://evil" }, { ...good, exp: now - 1 }, { ...good, sub: "" }]) {
  await assert.rejects(verifyIdToken(await token(bad), "harbor-lm", keys, now));
}
await assert.rejects(verifyIdToken(await token(good, "nope"), "harbor-lm", keys, now));
const t = await token(good);
await assert.rejects(verifyIdToken(t.slice(0, -4) + "AAAA", "harbor-lm", keys, now));

const res = await worker.fetch(new Request("https://x/v1/reports", { method: "POST", body: "{}" }), {});
assert.equal(res.status, 401);
console.log("report-worker tests passed");
