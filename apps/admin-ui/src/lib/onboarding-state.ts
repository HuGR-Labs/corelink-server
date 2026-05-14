/**
 * Onboarding wizard state.
 *
 * Persistence rules (CTRL-CRED-001):
 *  - Wizard step + tenant_id are persisted to sessionStorage so a user can
 *    refresh mid-flow.
 *  - PAT plaintext is NEVER persisted (sessionStorage or localStorage). It is
 *    held in memory only and zeroed on modal close.
 *  - Anything in localStorage is a bug.
 */

export type OnboardingStep =
  | "tenant"
  | "dpa"
  | "region-plan"
  | "billing"
  | "pat"
  | "done";

export const ALL_STEPS: readonly OnboardingStep[] = [
  "tenant",
  "dpa",
  "region-plan",
  "billing",
  "pat",
  "done",
] as const;

export interface PersistedState {
  step: OnboardingStep;
  tenantId: string | null;
}

const STORAGE_KEY = "corelink.onboarding.v1";

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

function safeStorage(): StorageLike | null {
  if (typeof window === "undefined") return null;
  try {
    return window.sessionStorage;
  } catch {
    return null;
  }
}

export function loadState(storage?: StorageLike | null): PersistedState {
  const s = storage === undefined ? safeStorage() : storage;
  if (!s) return { step: "tenant", tenantId: null };
  const raw = s.getItem(STORAGE_KEY);
  if (!raw) return { step: "tenant", tenantId: null };
  try {
    const parsed = JSON.parse(raw) as PersistedState;
    if (!ALL_STEPS.includes(parsed.step)) {
      return { step: "tenant", tenantId: null };
    }
    return parsed;
  } catch {
    return { step: "tenant", tenantId: null };
  }
}

export function saveState(
  state: PersistedState,
  storage?: StorageLike | null,
): void {
  const s = storage === undefined ? safeStorage() : storage;
  if (!s) return;
  // Defensive: refuse to persist anything that looks like a PAT.
  const json = JSON.stringify(state);
  if (/corelink_(prod|test)_/.test(json)) {
    throw new Error("Refusing to persist credential-shaped data");
  }
  s.setItem(STORAGE_KEY, json);
}

export function clearState(storage?: StorageLike | null): void {
  const s = storage === undefined ? safeStorage() : storage;
  if (!s) return;
  s.removeItem(STORAGE_KEY);
}

export function nextStep(step: OnboardingStep): OnboardingStep {
  const idx = ALL_STEPS.indexOf(step);
  if (idx < 0 || idx === ALL_STEPS.length - 1) return step;
  return ALL_STEPS[idx + 1];
}

export function prevStep(step: OnboardingStep): OnboardingStep {
  const idx = ALL_STEPS.indexOf(step);
  if (idx <= 0) return step;
  return ALL_STEPS[idx - 1];
}

/**
 * Returns the next step accounting for plan-driven skips.
 * Free plan skips the billing step (no payment method to collect).
 */
export function nextStepFor(
  step: OnboardingStep,
  ctx: { plan?: string | null },
): OnboardingStep {
  const candidate = nextStep(step);
  if (candidate === "billing" && ctx.plan === "free") {
    return nextStep(candidate);
  }
  return candidate;
}

/**
 * In-memory PAT holder. Module-scoped variable, NEVER serialized.
 */
let inMemoryPat: string | null = null;

export function setPlaintextPat(pat: string): void {
  inMemoryPat = pat;
}

export function getPlaintextPat(): string | null {
  return inMemoryPat;
}

export function clearPlaintextPat(): void {
  inMemoryPat = null;
}
