# NFT Minting Flow Diagram

This diagram displays the sequence of steps to tokenize name ownership as an NFT.

```mermaid
sequenceDiagram
    autonumber
    actor Owner as "Name Owner"
    participant NFT as "NFT Contract"
    participant Registry as "Registry Contract"

    Owner->>NFT: mint(token_id, owner_address, metadata_uri)
    Note over NFT: Check if token_id is already minted
    
    NFT->>Registry: resolve(name, now_unix)
    Registry-->>NFT: Return RegistryEntry (owner_address, etc.)
    
    Note over NFT: Verify token_id matches name and<br/>owner_address matches Registry owner
    Note over NFT: Save TokenRecord (owner, approved=None, metadata_uri)
    Note over NFT: Update TokenIds and OwnerTokens collections
    NFT-->>Owner: NFT Minted Successfully (Emit event)
```

## Detailed Flow Steps

1. **Mint Request**: The owner of an active name requests tokenization by calling the `mint` function on the `NftContract`.
2. **Availability Check**: The `NftContract` checks its storage to verify that the `token_id` (derived from the FQDN hash or name) has not already been tokenized.
3. **Canonical Ownership Check**: The `NftContract` performs a cross-contract lookup calling `Registry::resolve` with the FQDN.
4. **Ownership Verification**: The `NftContract` compares the returned owner address from the `RegistryEntry` to the caller's target address to ensure only the active owner of the name can tokenize it.
5. **Mint Completion**:
   - Saves a new `TokenRecord` indicating the owner, approvals, and token metadata URI.
   - Appends the ID to the global supply index and owner token collections.
   - Emits a standard mint event.
6. **Marketplace Integration**: Once minted, the owner can call `approve` or `transfer` on the NFT contract, enabling trustless secondary marketplace listing of the name token.
