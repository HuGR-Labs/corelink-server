// CTRL-PRIV-CONSENT dashboard route. Renders active consent rows from
// `/v1/consent/active`. Each row exposes view + withdraw actions.

import { ConsentDashboard } from "@/components/consent/ConsentDashboard";

export default function ConsentDashboardPage() {
  return <ConsentDashboard />;
}
