// Audit trail / history view. Paginated, with date-range + status + purpose
// filters. Renders from `/v1/consent/history`.

// ⛔ RETIRED — this page answers 404. `/v1/consent/history` has no handler, so
// there is no audit trail to paginate. Rationale + one-line reversal:
// `../retired.ts`.

import { ConsentHistory } from "@/components/consent/ConsentHistory";
import { assertConsentUiEnabled } from "../retired";

export default function ConsentHistoryPage() {
  assertConsentUiEnabled();
  return <ConsentHistory />;
}
