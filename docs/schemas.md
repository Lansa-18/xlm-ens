# Bridge Payload & Resolver Record Schemas

This document describes the data shapes exposed by the current contracts so integrators can build
against a stable surface without reading contract source.

## Bridge

### `BridgeRoute`

Stored under a `Route(<chain>)` persistent key in the Bridge contract.

Fields:

- `destination_chain` (string): Canonical chain identifier (e.g. `base`, `ethereum`, `arbitrum`).
- `destination_resolver` (string): Destination resolver identifier for the target chain.
- `gateway` (string): Gateway identifier used by the destination system.

### `build_message(name, chain) -> string`

Returns a JSON string shaped like:

```json
{
  "type": "xlm-ns-resolution",
  "name": "<fqdn>",
  "destination_chain": "<chain>",
  "resolver": "<destination_resolver>"
}
```

Notes:

- `name` must be a fully-qualified `.xlm` name (validated on-chain).
- `chain` must be a valid chain identifier (validated on-chain) and must have been registered.
- The returned message is deterministic for a given `(name, route)` pair.

### `build_reverse_message(address, primary_name, chain) -> string`

Returns a JSON string shaped like:

```json
{
  "type": "xlm-ns-reverse-resolution",
  "address": "<stellar_address>",
  "primary_name": "<fqdn>",
  "destination_chain": "<chain>",
  "resolver": "<destination_resolver>"
}
```

Notes:

- `address` and `primary_name` must be non-empty.
- `primary_name` must be a fully-qualified `.xlm` name (validated on-chain).
- `chain` must be a valid chain identifier (validated on-chain) and must have been registered.
- The returned message is deterministic for a given `(address, primary_name, route)` pair.

## Resolver

### `ResolutionRecord`

Stored under a `Forward(<name>)` persistent key in the Resolver contract.

Fields:

- `owner` (Address): Account authorized to mutate the record.
- `address` (string): Resolved target address for forward resolution.
- `text_records` (map<string, string>): Bounded set of text records (max `MAX_TEXT_RECORDS`).
- `updated_at` (u64): Unix timestamp supplied by the caller on write.

### Forward resolution

- `resolve(name) -> Option<ResolutionRecord>`
- Storage key: `Forward(<name>)`

### Reverse resolution

- `reverse(address) -> Option<string>`
- Storage keys:
  - `Primary(<address>)` (preferred when set)
  - `Reverse(<address>)` (fallback)

### Text records

- `set_text_record(name, caller, key, value, now_unix) -> Result<(), ResolverError>`
- Writes mutate the `text_records` map inside the `Forward(<name>)` record.

#### Text-record key normalization (#314)

Keys are validated on every write. A key is accepted when **all** of the
following hold:

- **Length**: 1–64 bytes (inclusive).
- **Characters**: lowercase ASCII letters `a-z`, digits `0-9`, dot `.`,
  dash `-`, or underscore `_`. Uppercase letters, spaces, and any other
  byte are rejected with `ResolverError::InvalidKey` (code 8).
- **Namespace convention**: reverse-DNS style namespacing (e.g.
  `com.twitter`, `org.did_key`) is the recommended pattern but is not
  enforced beyond the character set above.

Keys are stored **exactly as supplied**; the contract does not
automatically lowercase or otherwise transform the key. Callers are
responsible for normalising (e.g. lowercasing) before calling
`set_text_record`.

---

## Registrar

### `registration_status(label, now_unix) -> RegistrationStatus` (#311)

Returns the lifecycle status of a label (without the `.xlm` suffix):

| Variant | Meaning |
|---------|---------|
| `Unavailable` | Never registered, or no record exists. |
| `Active` | Registered and not yet expired (`now_unix <= expires_at`). |
| `GracePeriod` | Expired but within grace window; only current owner may renew. |
| `Claimable` | Past grace period; anyone may register. |
| `Reserved` | Blocked by the reserved-label list; cannot be registered. |

### `accounting_report() -> RegistrarMetrics` (#313)

Read-only aggregate view for operator reconciliation. Returns the same
`RegistrarMetrics` struct as `fee_metrics()`:

- `treasury_balance` (u64): cumulative stroops received (registrations + renewals, including overpayments).
- `total_registrations` (u64): lifetime successful registration count.
- `total_renewals` (u64): lifetime successful renewal count.

No write to storage is performed; this function is safe to call at any
time without side-effects.

### `quote_renewal(name, years, now_unix) -> RenewalQuote` (#220)

Read-only renewal quote for an already-registered name. Mirrors the fee and
expiry math in `renew` (renewing from the later of the current expiry or
`now_unix`) without mutating state or requiring auth.

`RenewalQuote`:

- `fee_stroops` (u64): total renewal fee (annual fee × `years`).
- `current_expiry_unix` (u64): the registration's current expiry.
- `extended_expiry_unix` (u64): expiry after the renewal would apply.
- `grace_period_ends_at` (u64): grace-period end derived from the extended expiry.
- `pricing` (`PricingBreakdown`): annual fee, duration, premium.

Returns `NotFound` if the name has never been registered.

### `pricing_policy_version() -> u32` (#217)

Read-only version of the registrar pricing table. Clients can poll this to
detect quote-policy changes without diffing individual quotes. The value is
bumped whenever the tiers/amounts in `price_for_label_length` change.

## Registry

### `name_state(name, now_unix) -> NameState` (#213)

Read-only lifecycle state of a name so callers can branch on the state directly
rather than inferring it from `resolve`/`register` errors.

| Variant | Meaning |
|---------|---------|
| `Missing` | No entry exists (also returned for unknown/invalid names). |
| `Active` | Registered and not yet expired (`now_unix <= expires_at`). |
| `GracePeriod` | Expired but within grace window (`expires_at < now_unix <= grace_period_ends_at`). |
| `Claimable` | Past grace period (`now_unix > grace_period_ends_at`); anyone may claim. |

No write to storage is performed.
