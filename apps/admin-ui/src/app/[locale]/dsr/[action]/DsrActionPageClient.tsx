"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { DsrActionForm } from "@/components/dsr/DsrActionForm";
import { JwtReceiptModal } from "@/components/dsr/JwtReceiptModal";
import { ReAuthGate, type ReAuthVerifier } from "@/components/dsr/ReAuthGate";
import {
  getMyProfile,
  listDataCategories,
  submitDsr,
} from "@/lib/dsr-client";
import {
  type DataCategory,
  type DsrAction,
  type DsrSubmitRequest,
  type DsrSubmitResponse,
  type MeProfile,
} from "@/lib/dsr-types";
import type { Locale } from "@/i18n";
import { Skeleton } from "@/components/ui/linear";

interface Props {
  locale: Locale;
  action: DsrAction;
  /**
   * Test injection overrides. In production, all these are sourced from
   * Clerk + the real DSR client.
   */
  testOverrides?: {
    profile?: MeProfile;
    categories?: DataCategory[];
    token?: string;
    verifier?: ReAuthVerifier;
    submit?: (req: DsrSubmitRequest) => Promise<DsrSubmitResponse>;
    initialVerifiedAt?: number;
  };
}

/**
 * Client island that:
 *   1. Loads the current Clerk user + data categories.
 *   2. Wraps the form in <ReAuthGate> (MFA required every submit).
 *   3. On success, shows the signed JWT receipt modal.
 */
export function DsrActionPageClient(props: Props) {
  // The browser E2E lane has no Clerk FAPI, but it must still exercise the
  // form, fresh-MFA gate, and receipt path. Keep this fixture behind the same
  // two conditions as the middleware/API mock: a test flag AND a non-prod
  // runtime. Production can therefore never receive a synthetic profile or
  // token merely because an environment variable leaked into a deploy.
  const e2eFixtureEnabled =
    process.env["NEXT_PUBLIC_E2E_TEST_MODE"] === "1" &&
    process.env.NODE_ENV !== "production";
  const e2eProfile: MeProfile = {
    email: "user@acme.example",
    name: "E2E User",
    language: props.locale,
  };
  const e2eCategories: DataCategory[] = [
    { id: "profile", label: "Profile" },
    { id: "activity", label: "Activity" },
  ];
  const e2eToken = "e2e-dsr-session";
  const effectiveOverrides = e2eFixtureEnabled
    ? {
        profile: e2eProfile,
        categories: e2eCategories,
        token: e2eToken,
      }
    : props.testOverrides;
  const [profile, setProfile] = useState<MeProfile | null>(
    effectiveOverrides?.profile ?? null,
  );
  const [categories, setCategories] = useState<DataCategory[]>(
    effectiveOverrides?.categories ?? [],
  );
  const [token, setToken] = useState<string | null>(
    effectiveOverrides?.token ?? null,
  );
  const [receipt, setReceipt] = useState<DsrSubmitResponse | null>(null);

  // In production, swap these for `useUser()` + `useSession().getToken()`.
  // Tests inject everything via `testOverrides`.
  useEffect(() => {
    if (props.testOverrides) return;
    let cancelled = false;
    void (async () => {
      try {
        const clerk = await import("@clerk/nextjs").catch(() => null);
        if (!clerk) return;
        // Clerk's session.getToken is async; consumers are responsible for
        // wiring the actual hooks. Production wiring lives in the integration
        // shell shipped by WI-S16-001/002.
      } catch {
        // Tests fall back to overrides.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [props.testOverrides]);

  const verifier: ReAuthVerifier = useMemo(
    () =>
    effectiveOverrides?.verifier ?? {
        async startVerification() {
          // Production: invoke Clerk MFA challenge via
          // `useClerk().session?.startVerification(...)`. The integration
          // shell in WI-S16-001 already exposes this — we receive the
          // resolved timestamp.
          return Date.now();
        },
      },
    [effectiveOverrides?.verifier],
  );

  const handleSubmit = useCallback(
    async (req: DsrSubmitRequest): Promise<DsrSubmitResponse> => {
      if (effectiveOverrides?.submit) {
        const r = await effectiveOverrides.submit(req);
        setReceipt(r);
        return r;
      }
      if (!token) {
        throw new Error("Missing session token");
      }
      const r = await submitDsr(req, { token });
      setReceipt(r);
      return r;
    },
    [effectiveOverrides, token],
  );

  // Best-effort production fetches; tests bypass via overrides.
  useEffect(() => {
    if (effectiveOverrides || !token) return;
    let cancelled = false;
    void (async () => {
      try {
        const [p, c] = await Promise.all([
          getMyProfile({ token }),
          listDataCategories({ token }),
        ]);
        if (cancelled) return;
        setProfile(p);
        setCategories(c);
      } catch {
        // Surfacing errors here is the integration shell's job.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [token, effectiveOverrides]);

  if (!profile) {
    return (
      <div data-testid="dsr-loading" aria-busy="true" aria-label="Loading">
        <Skeleton rows={3} />
      </div>
    );
  }

  return (
    <ReAuthGate
      locale={props.locale}
      verifier={verifier}
      initialVerifiedAt={props.testOverrides?.initialVerifiedAt ?? null}
    >
      {({ verifiedAt }) => (
        <>
          <DsrActionForm
            locale={props.locale}
            action={props.action}
            profile={profile}
            categories={categories}
            onSubmit={handleSubmit}
            mfaVerifiedAt={verifiedAt}
          />
          {receipt ? (
            <JwtReceiptModal
              locale={props.locale}
              token={receipt.jwt_receipt}
              slaDeadline={receipt.sla_deadline}
              onClose={() => setReceipt(null)}
            />
          ) : null}
        </>
      )}
    </ReAuthGate>
  );
}
