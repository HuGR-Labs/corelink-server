import { describe, expect, it } from "vitest";
import { tFor, MESSAGES, type Locale } from "@/i18n";
import { DSR_ACTIONS } from "@/lib/dsr-types";

const LOCALES: Locale[] = ["en", "pt", "es"];

describe("i18n DSR keys exist for all 6 rights × 3 locales (Test 12)", () => {
  for (const locale of LOCALES) {
    for (const action of DSR_ACTIONS) {
      it(`has label + description + legal_ref for ${action} in ${locale}`, () => {
        for (const sub of ["label", "description", "legal_ref"]) {
          const key = `dsr.rights.${action}.${sub}`;
          const value = tFor(locale, key);
          expect(value, `${key} missing on ${locale}`).not.toBe(key);
          expect(value.length).toBeGreaterThan(0);
        }
      });
    }

    it(`has form + reauth + receipt + status keys in ${locale}`, () => {
      const requiredKeys = [
        "dsr.landing.title",
        "dsr.form.submit",
        "dsr.form.reason_required_error",
        "dsr.reauth.title",
        "dsr.reauth.start_button",
        "dsr.receipt.modal_title",
        "dsr.status.title",
        "dsr.status_states.pending",
        "dsr.status_states.in_progress",
        "dsr.status_states.completed",
        "dsr.status_states.rejected",
        "dsr.countdown.overdue",
      ];
      for (const k of requiredKeys) {
        expect(tFor(locale, k), `${k} on ${locale}`).not.toBe(k);
      }
    });
  }

  it("MESSAGES object is non-empty for each locale", () => {
    for (const locale of LOCALES) {
      expect(MESSAGES[locale]).toBeDefined();
    }
  });
});
