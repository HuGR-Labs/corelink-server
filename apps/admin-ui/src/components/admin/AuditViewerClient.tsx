// Client wrapper around AuditViewer that constructs the AdminClient on the
// browser side, avoiding the RSC "class instance as prop" serializer error
// (wt-r3-7).

"use client";

import React from "react";
import { AdminClient } from "@/lib/admin-client";
import AuditViewer from "./AuditViewer";

const client = new AdminClient();

export function AuditViewerClient(): React.ReactElement {
  return <AuditViewer client={client} />;
}

export default AuditViewerClient;
