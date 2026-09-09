import { blake3 } from "@noble/hashes/blake3";
import { describe, expect, it } from "vitest";
import type {
  DurableObjectId,
  DurableObjectState,
  DurableObjectStorage,
  DurableObjectTransaction,
} from "@cloudflare/workers-types";
import { AuditHeadWitness, canonicalJson, type AppendRequest, type Env } from "../src/index.js";

const encoder = new TextEncoder();
const ZERO = "0".repeat(64);

function bytes(value: string): Uint8Array { return encoder.encode(value); }
function concat(...parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
  let offset = 0; for (const part of parts) { out.set(part, offset); offset += part.length; } return out;
}
function hex(value: Uint8Array): string { return Array.from(value, (byte) => byte.toString(16).padStart(2, "0")).join(""); }
function b64(value: Uint8Array): string { let raw = ""; for (const byte of value) raw += String.fromCharCode(byte); return btoa(raw); }
function u64be(value: number): Uint8Array {
  const out = new Uint8Array(8); let rest = BigInt(value);
  for (let index = 7; index >= 0; index -= 1) { out[index] = Number(rest & 255n); rest >>= 8n; }
  return out;
}

function state(store: Map<string, unknown>): DurableObjectState {
  const storage = {
    get: async <T>(key: string) => store.get(key) as T | undefined,
    put: async (key: string, value: unknown) => { store.set(key, value); },
    transaction: async (fn: (txn: DurableObjectTransaction) => Promise<void>) => fn({
      get: async <T>(key: string) => store.get(key) as T | undefined,
      put: async (key: string, value: unknown) => { store.set(key, value); },
    } as unknown as DurableObjectTransaction),
  } as unknown as DurableObjectStorage;
  return {
    storage,
    id: { toString: () => "partition", equals: (_id: DurableObjectId) => false } as DurableObjectId,
    blockConcurrencyWhile: async <T>(fn: () => Promise<T>) => fn(),
  } as unknown as DurableObjectState;
}

const env = {
  WITNESS_APPEND_TOKEN: "a".repeat(32),
  WITNESS_RECEIPT_SIGNING_SEED_HEX: "11".repeat(32),
  WITNESS_RECEIPT_KEY_ID: "1",
  WITNESS_ID: "security-witness-1",
} as Env;

function candidate(sequence: number, previous: string, suffix: string): { request: AppendRequest; hash: string } {
  const headJcs = canonicalJson({
    epoch_id: 0, epoch_ledger_hash: "2".repeat(64), epoch_ledger_sequence: 0,
    head_hash: suffix.repeat(64), head_message_version: 2, next_sequence: sequence,
    region: "enam", signing_key_id: 1, tenant_id: "018f22e2-7c61-7a37-8ef0-1ecf443f7182",
  });
  const signature = new Uint8Array(64);
  const headHash = hex(blake3(concat(bytes("corelink/audit-chain/head-record/v1\0"), u64be(bytes(headJcs).length), bytes(headJcs), signature)));
  const witnessJcs = canonicalJson({
    head_message_b64: b64(bytes(headJcs)), head_record_hash: headHash,
    head_signature_b64: b64(signature), previous_witness_hash: previous,
    region: "enam", tenant_id: "018f22e2-7c61-7a37-8ef0-1ecf443f7182", witness_sequence: sequence, witness_version: 1,
  });
  const hash = hex(blake3(concat(bytes("corelink/audit-chain/head-witness/v1\0"), bytes(witnessJcs))));
  return {
    request: {
      append_request_version: 1,
      expected_latest: sequence === 0 ? null : { witness_record_hash: previous, witness_sequence: sequence - 1 },
      witness_jcs_b64: b64(bytes(witnessJcs)),
    },
    hash,
  };
}

async function append(object: AuditHeadWitness, request: AppendRequest): Promise<Response> {
  return object.fetch(new Request("https://internal/append", {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(request),
  }));
}

describe("AuditHeadWitness", () => {
  it("atomically appends genesis and returns the identical receipt on retry", async () => {
    const object = new AuditHeadWitness(state(new Map()), env);
    const first = candidate(0, ZERO, "3");
    const committed = await append(object, first.request);
    expect(committed.status).toBe(201);
    const receipt = await committed.json();
    const replay = await append(object, first.request);
    expect(replay.status).toBe(201);
    expect(await replay.json()).toEqual(receipt);
  });

  it("rejects a competing successor and preserves the committed latest", async () => {
    const object = new AuditHeadWitness(state(new Map()), env);
    const genesis = candidate(0, ZERO, "3");
    const genesisReceipt = await (await append(object, genesis.request)).json();
    const winner = candidate(1, genesis.hash, "4");
    expect((await append(object, winner.request)).status).toBe(201);
    const loser = candidate(1, genesis.hash, "5");
    expect((await append(object, loser.request)).status).toBe(409);
    expect(await (await append(object, genesis.request)).json()).toEqual(genesisReceipt);
    const latest = await object.fetch(new Request("https://internal/latest", {
      method: "POST", body: JSON.stringify({
        challenge_b64: b64(new Uint8Array(32)), latest_request_version: 1,
        region: "enam", tenant_id: "018f22e2-7c61-7a37-8ef0-1ecf443f7182",
      }),
    }));
    expect(latest.status).toBe(200);
    expect((await latest.json() as Record<string, string>).latest_signature_b64).toHaveLength(88);
  });

  it("fails closed on malformed canonical evidence without writing", async () => {
    const store = new Map<string, unknown>();
    const object = new AuditHeadWitness(state(store), env);
    const genesis = candidate(0, ZERO, "3");
    genesis.request.witness_jcs_b64 = b64(bytes("{}"));
    expect((await append(object, genesis.request)).status).toBe(400);
    expect(store.size).toBe(0);
  });

  it("rejects BOM-prefixed exact evidence", async () => {
    const store = new Map<string, unknown>();
    const object = new AuditHeadWitness(state(store), env);
    const genesis = candidate(0, ZERO, "3");
    const decoded = atob(genesis.request.witness_jcs_b64);
    genesis.request.witness_jcs_b64 = btoa("\u00ef\u00bb\u00bf" + decoded);
    expect((await append(object, genesis.request)).status).toBe(400);
    expect(store.size).toBe(0);
  });

  it("pins witness identity at genesis", async () => {
    const store = new Map<string, unknown>();
    const object = new AuditHeadWitness(state(store), env);
    const genesis = candidate(0, ZERO, "3");
    expect((await append(object, genesis.request)).status).toBe(201);
    const changed = new AuditHeadWitness(state(store), { ...env, WITNESS_ID: "different-witness" });
    const response = await changed.fetch(new Request("https://internal/latest", {
      method: "POST", body: JSON.stringify({
        challenge_b64: b64(new Uint8Array(32)), latest_request_version: 1,
        region: "enam", tenant_id: "018f22e2-7c61-7a37-8ef0-1ecf443f7182",
      }),
    }));
    expect(response.status).toBe(503);
  });

  it("rejects noncanonical tenant and region latest requests", async () => {
    const object = new AuditHeadWitness(state(new Map()), env);
    for (const partition of [
      { tenant_id: "tenant-a", region: "enam" },
      { tenant_id: "018f22e2-7c61-7a37-8ef0-1ecf443f7182", region: "moon" },
    ]) {
      const response = await object.fetch(new Request("https://internal/latest", {
        method: "POST", body: JSON.stringify({
          challenge_b64: b64(new Uint8Array(32)), latest_request_version: 1, ...partition,
        }),
      }));
      expect(response.status).toBe(400);
    }
  });
});
