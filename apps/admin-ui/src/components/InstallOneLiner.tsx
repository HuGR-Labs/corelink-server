"use client";

import * as React from "react";

/**
 * InstallOneLiner — single copy-paste block that bootstraps the CoreLink CLI.
 *
 * Per Phase-0 PLG framework §4 step 4 + §5 row "Copy-paste one-liner install":
 *   curl -fsSL https://corelink-get.humangr.com | sh -s -- --token=corelink_pat_xxx --region=ord
 *
 * Token is shown only on this page once (CTRL-CRED-001 holds: the token is
 * delivered server-rendered by the parent and is **never** persisted to
 * localStorage — `apps/admin-ui/src/lib/onboarding-state.ts` already enforces
 * this for the wizard variant; the same constraint applies here).
 */
export interface InstallOneLinerProps {
  token: string;
  region: string;
  /** Override the install host for staging / preview environments. */
  installHost?: string;
}

export function InstallOneLiner(props: InstallOneLinerProps): React.ReactElement {
  const host = props.installHost ?? "corelink-get.humangr.com";
  const command = `curl -fsSL https://${host} | \\\n  sh -s -- --token=${props.token} --region=${props.region}`;
  const [copied, setCopied] = React.useState(false);

  async function copy(): Promise<void> {
    try {
      await navigator.clipboard.writeText(command);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard API can fail when the page is non-secure or perms revoked;
      // user can still select-all manually from the <code> block.
      setCopied(false);
    }
  }

  return (
    <div className="install-one-liner" data-testid="install-one-liner">
      <pre
        aria-label="CoreLink install command"
        data-testid="install-one-liner-cmd"
      >
        <code>{command}</code>
      </pre>
      <button
        type="button"
        onClick={copy}
        data-testid="install-one-liner-copy"
        aria-label="Copy install command to clipboard"
      >
        {copied ? "Copied" : "Copy"}
      </button>
    </div>
  );
}
