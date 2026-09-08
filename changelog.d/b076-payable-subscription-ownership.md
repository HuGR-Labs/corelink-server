- **Billing (B-076):** Stripe Checkout idempotency is tenant-scoped, pending
  payable sessions are durable guards, and D1 billing/tier writes require the
  exact session, customer, subscription, and correlation owner. A paid
  subscription can no longer be silently replaced by a second checkout.
