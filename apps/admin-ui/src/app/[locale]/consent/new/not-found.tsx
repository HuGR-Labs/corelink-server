/**
 * Route-local retirement response for consent capture.
 *
 * The page calls `notFound()` while the consent ledger is absent. Keeping the
 * marker in this segment's not-found boundary makes a 404 meaningful to the
 * e2e harness: an uncompiled route falls through to the generic 404 and is
 * rejected instead of being mistaken for this intentional retirement.
 */
export default function ConsentCaptureRetiredNotFound() {
  return (
    <main data-testid="consent-capture-retired" data-retired-route="consent-capture">
      <h1>Consent capture is temporarily unavailable</h1>
      <p>This route is retired until the consent ledger is available.</p>
    </main>
  );
}
