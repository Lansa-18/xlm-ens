# Registration Flow Diagram

This diagram displays the sequence of steps for registering a name, from initial pricing quote to configuring initial resolution records.

```mermaid
sequenceDiagram
    autonumber
    actor User as "User (Owner)"
    participant SDK as "CLI / SDK"
    participant Registrar as "Registrar Contract"
    participant Registry as "Registry Contract"
    participant Resolver as "Resolver Contract"
    participant Token as "Token Contract (XLM)"

    User->>SDK: Initiate register(label, years)
    SDK->>Registrar: quote_registration(label, years)
    Registrar-->>SDK: Return quote (fee, expiry timestamps)
    SDK-->>User: Display quote & request confirmation

    User->>SDK: Confirm registration (sign tx)
    SDK->>Registrar: register(label, owner, years, payment, now)
    
    Note over Registrar: Validate label format & years,<br/>check reserved list
    Note over Registrar: Check rate limits for owner
    
    Registrar->>Token: transfer(owner, treasury, fee_stroops)
    Token-->>Registrar: Success

    Registrar->>Registry: register(name, owner, None, None, now, expires_at, grace_period_ends_at)
    Note over Registry: Validate FQDN & availability
    Note over Registry: Save RegistryEntry in persistent storage
    Registry-->>Registrar: Success (Emit registry event)
    
    Registrar-->>SDK: Return receipt
    SDK-->>User: Registration successful!

    Note over User, Resolver: Initial Record Setup
    User->>SDK: Set initial address/records
    SDK->>Resolver: set_record(name, owner, address, now)
    Resolver->>Registry: resolve(name, now)
    Registry-->>Resolver: Return RegistryEntry (Verify owner == caller)
    Note over Resolver: Save ResolutionRecord & Primary (reverse) mapping
    Resolver-->>SDK: Success (Emit resolution event)
    SDK-->>User: Records successfully updated
```

## Detailed Flow Steps

1. **Quote Request**: The client requests a pricing calculation based on label length and registration years.
2. **Pricing Policy**: The `Registrar` calculates the annual fee tier (100 XLM for ≤3 characters, 25 XLM for 4-6, 10 XLM for ≥7 characters).
3. **Validation & Limits**:
   - Validation: Checks character validity (`a-z0-9-`) and length bounds.
   - Reserved List check: Ensures the label is not reserved.
   - Rate limiting: Rejects if the user has registered more than 5 names in 24 hours.
4. **Token Escrow**: The registration fee in stroops is transferred directly from the user to the treasury account.
5. **Registry Inception**: The `Registrar` makes a cross-contract invocation to `Registry::register`. The `Registry` writes a persistent `RegistryEntry` mapping the name to the owner and establishing expiration and grace period (90 days) boundaries.
6. **Initial Resolution Records**: The client invokes `Resolver::set_record` to setup forward address mapping. The `Resolver` performs an on-chain ownership check against the `Registry` via `Registry::resolve` before persisting the records and updating reverse lookup mappings.
