/**
 * Route-local fallback for the remaining consent dashboard paths.
 *
 * Concrete retired journeys have their own marker (`new` and `history`), but
 * every consent 404 should still be visibly tied to this retired surface.
 */
export default function ConsentRetiredNotFound() {
  return (
    <main data-testid="consent-retired" data-retired-route="consent">
      <h1>Consent management is temporarily unavailable</h1>
      <p>This route is retired until the consent ledger is available.</p>
    </main>
  );
}
