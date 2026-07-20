import { clerkSetup } from "@clerk/testing/playwright";

/**
 * Arms the Clerk Testing Token (bot-detection bypass) for the whole run. Reads
 * CLERK_PUBLISHABLE_KEY (the LIVE pk — construct as
 * `pk_live_<base64("clerk.corelink-app.humangr.com$")>`) + CLERK_SECRET_KEY
 * (sk_live) from the environment.
 */
export default async function globalSetup() {
  await clerkSetup({
    publishableKey: process.env["CLERK_PUBLISHABLE_KEY"],
    secretKey: process.env["CLERK_SECRET_KEY"],
  });
}
