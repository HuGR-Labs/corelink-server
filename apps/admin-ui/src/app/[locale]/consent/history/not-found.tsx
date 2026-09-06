/**
 * Route-local retirement response for consent history/withdrawal.
 *
 * See the capture boundary for why this marker is deliberately segment-local:
 * a generic Next 404 must not satisfy the intentional-retirement probe.
 */
export default function ConsentHistoryRetiredNotFound() {
  return (
    <main data-testid="consent-history-retired" data-retired-route="consent-history">
      <h1>Consent withdrawal is temporarily unavailable</h1>
      <p>This route is retired until the consent ledger is available.</p>
    </main>
  );
}
