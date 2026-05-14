// Audit trail / history view. Paginated, with date-range + status + purpose
// filters. Renders from `/v1/consent/history`.

import { ConsentHistory } from "@/components/consent/ConsentHistory";

export default function ConsentHistoryPage() {
  return <ConsentHistory />;
}
