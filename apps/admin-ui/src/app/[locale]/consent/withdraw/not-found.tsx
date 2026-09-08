/**
 * Route-local fallback for consent withdrawal by id.
 */
export default function ConsentWithdrawRetiredNotFound() {
  return (
    <main data-testid="consent-withdraw-retired" data-retired-route="consent-withdraw">
      <h1>Consent withdrawal is temporarily unavailable</h1>
      <p>This route is retired until the consent ledger is available.</p>
    </main>
  );
}
