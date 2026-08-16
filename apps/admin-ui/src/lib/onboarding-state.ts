/**
 * Onboarding state — collapsed signup model (Phase-0 PLG framework §4).
 *
 * Before this refactor the wizard had six gated steps (tenant → dpa →
 * region-plan → billing → pat → done). The collapsed flow has just two:
 * the Clerk sign-up form, then `/welcome`. Tenant + region + plan + PAT
 * are provisioned server-side on the Clerk `user.created` webhook.
 *
 * What survives in this module:
 *  - The in-memory PAT holder (`setPlaintextPat` / `getPlaintextPat` /
 *    `clearPlaintextPat`) is still the **only** approved channel for
 *    handling a freshly-issued PAT inside the admin-ui process. The
 *    /welcome page reads the plaintext from a one-shot Clerk session
 *    claim and stashes it here in memory; nothing else may persist it.
 *  - The CTRL-CRED-001 guard (`saveState` refusing credential-shaped
 *    JSON, no localStorage anywhere) is preserved as a defensive guard
 *    for any future ephemeral wizard state.
 *
 * What is gone:
 *  - The `OnboardingStep` graph, `ALL_STEPS`, `nextStep*`/`prevStep*`
 *    helpers, and the free-plan-skip branch. The new flow has a single
 *    landing screen so there are no step transitions to model.
 */

export interface PersistedState {
  /**
   * Tenant id of the freshly-provisioned workspace, if the webhook has
   * landed by the time the user reaches /welcome. Stored in
   * sessionStorage so a page refresh during the activation wait keeps
   * the SSE subscription pointed at the right tenant.
   */
  tenantId: string | null;
}

const STORAGE_KEY = "corelink.onboarding.v2";

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
  if (!s) return { tenantId: null };
  const raw = s.getItem(STORAGE_KEY);
  if (!raw) return { tenantId: null };
  try {
    const parsed = JSON.parse(raw) as PersistedState;
    return { tenantId: typeof parsed.tenantId === "string" ? parsed.tenantId : null };
  } catch {
    return { tenantId: null };
  }
}

export function saveState(
  state: PersistedState,
  storage?: StorageLike | null,
): void {
  const s = storage === undefined ? safeStorage() : storage;
  if (!s) return;
  const json = JSON.stringify(state);
  // CTRL-CRED-001 guard: refuse to write anything that looks like a PAT.
  if (/corelink_(pat|ci|ro)_/.test(json)) {
    throw new Error("Refusing to persist credential-shaped data");
  }
  s.setItem(STORAGE_KEY, json);
}

export function clearState(storage?: StorageLike | null): void {
  const s = storage === undefined ? safeStorage() : storage;
  if (!s) return;
  s.removeItem(STORAGE_KEY);
}

// ──────────────────────────────────────────────────────────────────────────
// In-memory PAT holder — module-scoped variable, NEVER serialized.
// ──────────────────────────────────────────────────────────────────────────

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
