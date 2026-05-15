// Client wrapper that instantiates AdminClient inside the browser (wt-r3-7).
//
// Server components cannot pass class instances as props to client components
// in Next 15 RSC — the serializer rejects non-plain objects. This wrapper
// holds the construction on the client.

"use client";

import React from "react";
import { AdminClient } from "@/lib/admin-client";
import TenantSearch from "./TenantSearch";

const client = new AdminClient();

export function TenantSearchClient(): React.ReactElement {
  return <TenantSearch client={client} />;
}

export default TenantSearchClient;
