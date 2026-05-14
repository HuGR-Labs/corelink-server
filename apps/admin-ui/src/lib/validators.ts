/**
 * Pure validators used by onboarding forms.
 *
 * Kept dependency-free so they can be unit tested without booting Next.js
 * or Clerk. Imported by both server actions and client components.
 */

export type ValidationResult =
  | { ok: true }
  | { ok: false; reason: "too_short" | "too_long" | "invalid_chars" | "empty" };

const TENANT_NAME_PATTERN = /^[a-zA-Z0-9-]+$/;

export function validateTenantName(name: string): ValidationResult {
  if (name.length === 0) {
    return { ok: false, reason: "empty" };
  }
  if (name.length < 3) {
    return { ok: false, reason: "too_short" };
  }
  if (name.length > 64) {
    return { ok: false, reason: "too_long" };
  }
  if (!TENANT_NAME_PATTERN.test(name)) {
    return { ok: false, reason: "invalid_chars" };
  }
  return { ok: true };
}

export const SUPPORTED_REGIONS = ["us-east", "eu-west", "sa-east"] as const;
export type Region = (typeof SUPPORTED_REGIONS)[number];

export const SUPPORTED_PLANS = ["free", "starter", "pro", "enterprise"] as const;
export type Plan = (typeof SUPPORTED_PLANS)[number];

export function isFreeplan(plan: Plan): boolean {
  return plan === "free";
}

export const SUPPORTED_LOCALES = ["en", "pt", "es"] as const;
export type Locale = (typeof SUPPORTED_LOCALES)[number];

export const PAT_EXPIRY_OPTIONS = [30, 90, 365] as const;
export type PatExpiryDays = (typeof PAT_EXPIRY_OPTIONS)[number];

export const PAT_SCOPES = ["read-only", "read-write", "admin"] as const;
export type PatScope = (typeof PAT_SCOPES)[number];

export interface PatCreateInput {
  label: string;
  scope: PatScope;
  expiryDays: PatExpiryDays;
}

export function validatePatInput(input: PatCreateInput): ValidationResult {
  if (input.label.trim().length === 0) {
    return { ok: false, reason: "empty" };
  }
  if (input.label.length > 64) {
    return { ok: false, reason: "too_long" };
  }
  return { ok: true };
}
