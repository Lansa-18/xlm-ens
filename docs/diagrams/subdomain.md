# Subdomain Flow Diagram

This diagram displays the sequence of steps for registering parent domains, managing controllers, and creating subdomains.

```mermaid
sequenceDiagram
    autonumber
    actor ParentOwner as "Parent Owner"
    actor Controller as "Controller / Agent"
    actor SubOwner as "Subdomain Owner"
    participant Subdomain as "Subdomain Contract"
    participant Registry as "Registry Contract"

    ParentOwner->>Subdomain: register_parent(parent, parent_owner_address)
    Note over Subdomain: Validate FQDN format
    Note over Subdomain: Save ParentDomain record
    Subdomain-->>ParentOwner: Parent Registered

    ParentOwner->>Subdomain: add_controller(parent, owner, controller_address)
    Note over Subdomain: Verify caller is parent owner
    Note over Subdomain: Add controller to list
    Subdomain-->>ParentOwner: Controller Added

    Controller->>Subdomain: create(label, parent, controller, subdomain_owner_address, now)
    Note over Subdomain: Verify caller is parent owner or controller
    Note over Subdomain: Build child FQDN (label.parent)
    Note over Subdomain: Check subdomain availability
    Note over Subdomain: Save SubdomainRecord & update index mappings
    Subdomain-->>Controller: Subdomain Created (Emit event)

    Note over SubOwner: Subdomain Management
    SubOwner->>Subdomain: transfer(fqdn, sub_owner, new_sub_owner)
    Note over Subdomain: Verify caller is current subdomain owner
    Note over Subdomain: Update owner & update index mappings
    Subdomain-->>SubOwner: Transfer complete
```

## Detailed Flow Steps

1. **Register Parent**: The parent domain owner must register the parent domain (e.g. `domain.xlm`) in the `Subdomain` contract to enable sub-namespace delegation.
2. **Delegate Authority**: The parent owner can whitelist other addresses as controllers, allowing them to create subdomains under that parent domain.
3. **Subdomain Creation**: A parent owner or controller calls `create`. The contract validates the sub-label, generates the fully-qualified domain name (e.g. `sub.domain.xlm`), and stores a new `SubdomainRecord` mapping the child name to its owner.
4. **Subdomain Isolation**: Subdomain lifecycles and records are managed independently within the `Subdomain` contract's namespace storage, separate from the primary `Registry` contract database.
