import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { blake3 } from "@noble/hashes/blake3";
import { canonicalJson, type AppendRequest } from "../src/index.js";

const APP = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const DIST = resolve(APP, "dist/index.js");
const AUTH = "a".repeat(32);
let mf: import("miniflare").Miniflare;

const bytes = (value: string) => new TextEncoder().encode(value);
const concat = (...parts: Uint8Array[]) => {
  const out = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
  let offset = 0; for (const part of parts) { out.set(part, offset); offset += part.length; } return out;
};
const hex = (value: Uint8Array) => Array.from(value, (byte) => byte.toString(16).padStart(2, "0")).join("");
const b64 = (value: Uint8Array) => { let raw = ""; for (const byte of value) raw += String.fromCharCode(byte); return btoa(raw); };
const u64be = (value: number) => { const out = new Uint8Array(8); let n = BigInt(value); for (let i = 7; i >= 0; i -= 1) { out[i] = Number(n & 255n); n >>= 8n; } return out; };

function genesis(suffix: string): { body: string; hash: string } {
  const head = canonicalJson({
    epoch_id: 0, epoch_ledger_hash: "2".repeat(64), epoch_ledger_sequence: 0,
    head_hash: suffix.repeat(64), head_message_version: 2, next_sequence: 0,
    region: "enam", signing_key_id: 1, tenant_id: "018f22e2-7c61-7a37-8ef0-1ecf443f7182",
  });
  const signature = new Uint8Array(64);
  const headHash = hex(blake3(concat(bytes("corelink/audit-chain/head-record/v1\0"), u64be(bytes(head).length), bytes(head), signature)));
  const witness = canonicalJson({
    head_message_b64: b64(bytes(head)), head_record_hash: headHash,
    head_signature_b64: b64(signature), previous_witness_hash: "0".repeat(64),
    region: "enam", tenant_id: "018f22e2-7c61-7a37-8ef0-1ecf443f7182", witness_sequence: 0, witness_version: 1,
  });
  const hash = hex(blake3(concat(bytes("corelink/audit-chain/head-witness/v1\0"), bytes(witness))));
  const request: AppendRequest = { append_request_version: 1, expected_latest: null, witness_jcs_b64: b64(bytes(witness)) };
  return { body: canonicalJson(request), hash };
}

beforeAll(async () => {
  execFileSync(resolve(APP, "node_modules/.bin/wrangler"), ["deploy", "--dry-run", "--outdir", "dist"], { cwd: APP, stdio: "inherit" });
  const { Miniflare } = await import("miniflare");
  mf = new Miniflare({
    scriptPath: DIST, modules: true, modulesRoot: resolve(APP, "dist"),
    compatibilityDate: "2026-07-17",
    durableObjects: { HEAD_WITNESS: "AuditHeadWitness" },
    bindings: {
      WITNESS_APPEND_TOKEN: AUTH,
      WITNESS_RECEIPT_SIGNING_SEED_HEX: "11".repeat(32),
      WITNESS_RECEIPT_KEY_ID: "1",
      WITNESS_ID: "security-witness-1",
    },
  });
});

afterAll(async () => { await mf?.dispose(); });

describe("real-workerd witness linearization", () => {
  it("commits exactly one concurrent successor and preserves retry/latest", async () => {
    const candidates = [genesis("3"), genesis("4")];
    const responses = await Promise.all(candidates.map((candidate) => mf.dispatchFetch("https://witness/v1/audit-chain/head-witness/compare-and-append", {
      method: "POST",
      headers: { authorization: `Bearer ${AUTH}`, "content-type": "application/jcs+json", "idempotency-key": candidate.hash },
      body: candidate.body,
    })));
    expect(responses.map((response: { status: number }) => response.status).sort()).toEqual([201, 409]);
    const winnerIndex = responses.findIndex((response: { status: number }) => response.status === 201);
    const committed = await responses[winnerIndex].json();
    const winner = candidates[winnerIndex];
    const replay = await mf.dispatchFetch("https://witness/v1/audit-chain/head-witness/compare-and-append", {
      method: "POST",
      headers: { authorization: `Bearer ${AUTH}`, "content-type": "application/jcs+json", "idempotency-key": winner.hash },
      body: winner.body,
    });
    expect(replay.status).toBe(201);
    expect(await replay.json()).toEqual(committed);

    const latestRequest = canonicalJson({
      challenge_b64: b64(new Uint8Array(32)), latest_request_version: 1,
      region: "enam", tenant_id: "018f22e2-7c61-7a37-8ef0-1ecf443f7182",
    });
    const latest = await mf.dispatchFetch("https://witness/v1/audit-chain/head-witness/latest", {
      method: "POST",
      headers: { authorization: `Bearer ${AUTH}`, "content-type": "application/jcs+json" },
      body: latestRequest,
    });
    expect(latest.status).toBe(200);
    const latestEnvelope = await latest.json() as { latest_jcs_b64: string };
    const signedLatest = JSON.parse(new TextDecoder().decode(Uint8Array.from(atob(latestEnvelope.latest_jcs_b64), (char) => char.charCodeAt(0))));
    expect(signedLatest.latest.witness_record_hash).toBe(winner.hash);
  });
});
