import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";

/**
 * Server-side boundary for the interactive audit visualization. The page is a
 * client component (it verifies proofs in the browser), so middleware alone is
 * not sufficient: direct SSR/render requests must be rejected before the page
 * can bootstrap tenant-scoped fetches.
 */
export default function AuditVisualizationLayout({
  children,
}: {
  children: React.ReactNode;
}): React.ReactElement {
  return <CustomerGuard>{children}</CustomerGuard>;
}
