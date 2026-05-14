/**
 * CoreLink JS/TS WASM quickstart — @corelink/client.
 *
 * Requires:
 *   npm install @corelink/client
 *   export CORELINK_PAT=<your-pat>
 *
 * Run:
 *   npx ts-node quickstart.ts
 *   # or
 *   node --experimental-vm-modules dist/quickstart.js
 */

import { CoreLinkClient } from "@corelink/client";

async function main(): Promise<void> {
  const pat = process.env.CORELINK_PAT;
  if (!pat) {
    console.error("ERROR: CORELINK_PAT environment variable not set");
    process.exit(1);
  }

  // clientVerify defaults to true per CTRL-CAS-002.
  // Pass clientVerify: false to opt out (warning logged).
  const client = new CoreLinkClient({
    pat,
    tenantId: "acme-corp",
    // clientVerify: true  // default; shown for clarity
  });

  console.log(`_clientVerifyEnabled: ${client._clientVerifyEnabled}`);

  // Put
  const data = new TextEncoder().encode("hello from CoreLink JS/TS SDK");
  const digest = await client.put(data);
  console.log(`Uploaded:  blake3:${digest}`);

  // Get (client-verify enforced by Rust single truth via WASM)
  const downloaded = await client.get(digest);
  console.log(`Downloaded: ${downloaded.length} bytes`);

  // Stat
  const stat = await client.stat(digest);
  console.log(
    `Stat:      digest=${stat.digest.slice(0, 8)}... ` +
    `size=${stat.sizeBytes} exists=${stat.exists}`
  );

  console.log("OK — JS/TS SDK quickstart complete.");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
