// WI-S16-005 — canonical 403 page for admin scope failures.

import React from "react";

export interface ForbiddenPageProps {
  searchParams?: { reason?: string };
}

export default function ForbiddenPage({
  searchParams,
}: ForbiddenPageProps): React.ReactElement {
  const reason = searchParams?.reason ?? "operator role required";
  return (
    <main aria-labelledby="forbidden-heading">
      <h1 id="forbidden-heading">403 — forbidden</h1>
      <p>{reason}</p>
      <p>
        Admin pages require the <code>corelink-admin</code> Clerk org role. If you
        believe you should have access, contact your security lead.
      </p>
    </main>
  );
}
