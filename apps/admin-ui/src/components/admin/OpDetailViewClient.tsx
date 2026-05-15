// Client wrapper around OpDetailView that constructs the AdminClient on the
// browser side, avoiding the RSC "class instance as prop" serializer error
// (wt-r3-7).

"use client";

import React from "react";
import type { AdminOp } from "@/lib/types";
import { AdminClient } from "@/lib/admin-client";
import OpDetailView from "./OpDetailView";

const client = new AdminClient();

export interface OpDetailViewClientProps {
  initialOp: AdminOp;
  currentUserId: string | null;
  mfaFresh: boolean;
}

export function OpDetailViewClient({
  initialOp,
  currentUserId,
  mfaFresh,
}: OpDetailViewClientProps): React.ReactElement {
  return (
    <OpDetailView
      initialOp={initialOp}
      currentUserId={currentUserId}
      mfaFresh={mfaFresh}
      client={client}
    />
  );
}

export default OpDetailViewClient;
