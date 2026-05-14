/**
 * DPA scroll tracking helper.
 *
 * The "I accept" button MUST remain disabled until the user has scrolled
 * to the bottom of the DPA. We treat "within 8 pixels of the bottom" as
 * scrolled-to-end to allow for sub-pixel rounding and trailing whitespace.
 */

export interface ScrollSnapshot {
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
}

const SLACK_PX = 8;

export function hasScrolledToEnd(snap: ScrollSnapshot): boolean {
  const remaining = snap.scrollHeight - (snap.scrollTop + snap.clientHeight);
  return remaining <= SLACK_PX;
}

export interface DpaAcceptanceRecord {
  version: string;
  locale: string;
  acceptedAt: string; // ISO-8601, browser-side capture
  noticeTextHash: string; // sha-256 hex of canonical DPA text
}

export function buildAcceptanceRecord(
  version: string,
  locale: string,
  noticeTextHash: string,
  now: Date = new Date(),
): DpaAcceptanceRecord {
  return {
    version,
    locale,
    acceptedAt: now.toISOString(),
    noticeTextHash,
  };
}
