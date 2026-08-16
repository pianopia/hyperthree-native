# Game sales and creator payouts plan

This document plans the commerce layer; it does not enable live payments or
store secret keys in this repository.

Stripe Connect is the proposed payment rail because it supports connected
account onboarding, payments, payouts, platform fees, and seller account
management. See the [Stripe Connect overview](https://docs.stripe.com/connect/how-connect-works).

## Product model

The platform has three actors:

1. **Player**: purchases a game or an in-game entitlement.
2. **Creator**: publishes a game and receives the creator share.
3. **Pianopia platform**: owns catalog, checkout, entitlement, fee policy,
   support, and reconciliation workflows.

The native game runtime never handles card data. The web control plane owns
catalog, checkout, account onboarding, webhook processing, and dashboards.

## Initial Connect choice

The provisional first implementation is:

- Create one Stripe Connected Account per creator.
- Start with Express Dashboard access for the fastest compliant onboarding and
  payout visibility. The Express Dashboard lets connected accounts view their
  balance, upcoming payouts, and earnings; later we can move to a fully
  embedded experience where the platform owns the UI.
- Use destination charges with a versioned `application_fee_amount` policy for
  the marketplace flow, subject to the merchant-of-record, refund, tax, and
  negative-balance decision before live mode.
- Treat Stripe's actual processing, Connect, currency-conversion, and payout
  charges as ledger inputs. Do not hard-code a guessed Stripe fee formula.

Stripe documents that charge type affects where funds settle and who bears
refunds/chargebacks, so the final charge model must be approved with the
platform's legal and accounting owners. See [Connect charge types](https://docs.stripe.com/connect/charges).

## Money flow

```text
Creator onboarding
  -> charges_enabled / payouts_enabled
  -> Game published with price and fee schedule version
  -> Player pays through Checkout
  -> verified webhook records payment and entitlement
  -> creator share and application fee become ledger entries
  -> Stripe balance settles and payout is scheduled
  -> creator sees gross, Stripe fees, platform fee, net, and payout status
```

Payment completion is webhook-driven. The client redirect is only a UX hint;
entitlements are granted after the server verifies the relevant Stripe event.

## Data model

The commerce service should own an append-only ledger with idempotent webhook
processing.

| Record | Important fields |
| --- | --- |
| `creator_accounts` | user ID, Stripe account ID, country, capabilities, `charges_enabled`, `payouts_enabled`, requirements status |
| `games` | owner, slug, title, current release, visibility, refund policy |
| `releases` | immutable artifact, platform, checksum, runtime version, price IDs |
| `fee_schedules` | version, currency/region, percentage, fixed component, effective time, refund rule |
| `orders` | player, game/release, Checkout Session, currency, gross amount, status |
| `ledger_entries` | order, Stripe object, gross, processing fee, application fee, creator net, currency, balance transaction |
| `entitlements` | player, game/release, source order, granted/revoked timestamps |
| `payouts` | creator, Stripe payout ID, amount, currency, status, arrival date, failure reason |
| `webhook_events` | Stripe event ID, type, payload hash, received/processed timestamps, retry state |

All monetary values are integer minor units plus an ISO currency. Every event
handler is idempotent by Stripe event ID and business operation key.

## Fee and payout policy

The admin dashboard will manage a versioned fee schedule instead of embedding
rates in game code. A schedule can define:

- platform percentage fee;
- fixed platform fee by currency;
- promotional or per-game overrides;
- effective-from timestamp;
- refund and chargeback treatment;
- minimum creator payout threshold and payout hold period.

The creator view must show the expected split before publication and the actual
settled split after reconciliation. Stripe's application-fee reporting and
balance transactions are the source of truth for actual settlement. See
[application fees](https://docs.stripe.com/connect/marketplace/tasks/app-fees).

Payouts should use Stripe Connect payout controls and status events. Do not
build an internal bank-transfer system for the first release.

## Dashboards

### Creator dashboard

- Connect onboarding and missing requirements;
- game publishing and release history;
- gross sales, refunds, disputes, Stripe fees, platform fees, and net earnings;
- upcoming and completed payouts;
- downloadable monthly statements;
- Express Dashboard link initially, with embedded Connect components as the
  product matures.

### Platform admin dashboard

- creator/KYC and capability status;
- catalog and release moderation;
- orders, refunds, disputes, chargebacks, and entitlement correction;
- fee schedule versions and preview calculator;
- payout holds, failed payouts, reserves, and negative balances;
- webhook/reconciliation health and daily balance reports.

Stripe also supports embedded account management, payments, payouts, balances,
and documents components for a custom platform UI; evaluate these after the
Express-based MVP. See [fully embedded Connect](https://docs.stripe.com/connect/build-full-embedded-integration).

## Security and launch gates

- Stripe secret keys stay server-side; the native client receives no secret.
- Verify webhook signatures, persist event IDs, and make handlers retry-safe.
- Never store raw card data; use Checkout/Payment Element and Stripe tokens.
- Add refund, dispute, fraud, tax, KYC/AML, privacy, and terms-of-sale policies.
- Decide who is merchant of record and who bears negative balances before live
  charges. This is a business/legal decision, not a runtime default.
- Start in a Stripe test environment, then a restricted pilot, then staged live
  rollout with payout reconciliation and manual support coverage.

