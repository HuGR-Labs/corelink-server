"use server";

import { apiPost } from "@/lib/api-client";
import {
  validateTenantName,
  validatePatInput,
  type PatScope,
  type PatExpiryDays,
  type Region,
  type Plan,
} from "@/lib/validators";

/**
 * Resolve the Clerk session token at the server boundary.
 *
 * In production this delegates to `auth()` from `@clerk/nextjs/server`.
 * We isolate the call so unit tests can stub it.
 */
async function getSessionToken(): Promise<string> {
  // Lazy import so test environments that lack Clerk wiring don't crash.
  const mod = await import("@clerk/nextjs/server").catch(() => null);
  if (!mod) throw new Error("Auth provider not initialized");
  const session = await (mod as { auth: () => Promise<{ getToken: () => Promise<string | null> }> }).auth();
  const token = await session.getToken();
  if (!token) throw new Error("No active session");
  return token;
}

export interface TenantCreated {
  id: string;
  name: string;
}

export async function createTenantAction(input: {
  name: string;
  legalName: string;
  taxId?: string;
}): Promise<TenantCreated> {
  const v = validateTenantName(input.name);
  if (!v.ok) {
    throw new Error(`tenant_name_invalid:${v.reason}`);
  }
  const token = await getSessionToken();
  return apiPost<TenantCreated>(
    "/v1/tenants",
    {
      name: input.name,
      legal_name: input.legalName,
      tax_id: input.taxId ?? null,
    },
    { token },
  );
}

export interface DpaAccepted {
  audit_event_id: string;
  dpa_version: string;
  dpa_locale: string;
  dpa_accepted_at: string;
}

export async function acceptDpaAction(input: {
  tenantId: string;
  dpaVersion: string;
  dpaLocale: string;
  noticeTextHash: string;
}): Promise<DpaAccepted> {
  const token = await getSessionToken();
  return apiPost<DpaAccepted>(
    `/v1/tenants/${encodeURIComponent(input.tenantId)}/dpa-accept`,
    {
      dpa_version: input.dpaVersion,
      dpa_locale: input.dpaLocale,
      notice_text_hash: input.noticeTextHash,
    },
    { token },
  );
}

export async function configureTenantAction(input: {
  tenantId: string;
  region: Region;
  plan: Plan;
}): Promise<{ ok: true }> {
  const token = await getSessionToken();
  return apiPost<{ ok: true }>(
    `/v1/tenants/${encodeURIComponent(input.tenantId)}/configure`,
    { region: input.region, plan: input.plan },
    { token },
  );
}

/**
 * Stripe Checkout Session — PLG defer-billing pattern (Phase 0.C).
 *
 * Signup wizard no longer asks for a payment method. Money question
 * deferred to upgrade-click via Stripe-hosted Checkout Session. The
 * backend `POST /v1/onboarding/tier-select` enforces the
 * INV-ONBOARD-DPA-FIRST D1 lock (DPA acceptance row MUST exist before
 * a Stripe Checkout session is minted) and returns a Stripe-hosted
 * `checkout.session.url` we redirect to. Tier activation (set
 * `tenants.plan = 'pro'`) happens server-side when the corresponding
 * `checkout.session.completed` webhook lands on
 * `/v1/billing/stripe-webhook` (idempotent via canonical Stripe
 * `evt_*` event id — see `corelink-tier-selection/src/ledger.rs`).
 *
 * See `specs/_audits/2026-05-27-phase-0-execution-plan.md` §2.C +
 * `specs/_audits/2026-05-27-plg-onboarding-framework.md` §4.
 */
export interface CheckoutSessionRequest {
  /** Canonical tier id (`free` | `starter` | `team` | `pro` | `enterprise`). */
  tier: string;
  success_url: string;
  cancel_url: string;
}

export interface CheckoutSessionResponse {
  /** Stripe-hosted Checkout URL — client redirects via `Location: 303`. */
  checkout_url: string;
  /** Stripe `cs_*` session id (echoed back in success_url for analytics). */
  session_id: string;
}

export async function createCheckoutSessionAction(
  input: CheckoutSessionRequest,
): Promise<CheckoutSessionResponse> {
  const token = await getSessionToken();
  return apiPost<CheckoutSessionResponse>(
    "/v1/onboarding/tier-select",
    {
      tier: input.tier,
      success_url: input.success_url,
      cancel_url: input.cancel_url,
    },
    { token },
  );
}

export interface PatIssued {
  id: string;
  label: string;
  /**
   * Plaintext token, shown to the user ONCE. The server is contractually
   * obligated to never return this on subsequent fetches (CTRL-CRED-001).
   */
  plaintext: string;
}

export async function createPatAction(input: {
  label: string;
  scope: PatScope;
  expiryDays: PatExpiryDays;
}): Promise<PatIssued> {
  const v = validatePatInput(input);
  if (!v.ok) {
    throw new Error(`pat_input_invalid:${v.reason}`);
  }
  const token = await getSessionToken();
  return apiPost<PatIssued>(
    "/v1/pats",
    {
      label: input.label,
      scope: input.scope,
      expiry_days: input.expiryDays,
    },
    { token },
  );
}
