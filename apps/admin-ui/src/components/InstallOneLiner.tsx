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
 *
 * Linear kit: rendered with the frozen `.lin-code` terminal block (dark in both
 * themes per the Raycast/Linear rule) + the kit ghost copy button. Testids are
 * preserved for the welcome-flow e2e specs.
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
  const timer = React.useRef<ReturnType<typeof setTimeout> | null>(null);

  React.useEffect(() => {
    return () => {
      if (timer.current != null) clearTimeout(timer.current);
    };
  }, []);

  async function copy(): Promise<void> {
    try {
      await navigator.clipboard.writeText(command);
      setCopied(true);
      if (timer.current != null) clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard API can fail when the page is non-secure or perms revoked;
      // user can still select-all manually from the <code> block.
      setCopied(false);
    }
  }

  return (
    <div className="lin-code" data-testid="install-one-liner">
      <button
        type="button"
        className="lin-code__copy lin-btn lin-btn--ghost lin-btn--sm"
        onClick={copy}
        data-testid="install-one-liner-copy"
        aria-label="Copy install command to clipboard"
      >
        {copied ? "Copied" : "Copy"}
      </button>
      <pre aria-label="CoreLink install command" data-testid="install-one-liner-cmd">
        <code>{command}</code>
      </pre>
    </div>
  );
}
