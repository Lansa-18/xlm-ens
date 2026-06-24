# xlm-ens Disaster Recovery Plan

This document outlines the procedures for recovering from various disaster scenarios involving the xlm-ens smart contracts.

## 1. State Backup

### 1.1. Automated Snapshots

Automated scripts will be used to snapshot critical state at regular intervals. This includes:

*   Registry ownership
*   Resolver records
*   Auction state

These snapshots will be taken by replaying events or querying the ledger directly.

### 1.2. Backup Scripts

Backup scripts will be located in `ops/scripts/backup/`.

*   `ops/scripts/backup/snapshot.sh`: Takes a complete snapshot of the current state.
*   `ops/scripts/backup/restore.sh`: Restores state from a snapshot.

## 2. Emergency Procedures

### 2.1. Contract Pause

In the event of a critical bug or exploit, the contracts can be paused. This will prevent any state changes until the issue is resolved.

*   `ops/scripts/emergency/pause.sh`: Pauses all contracts.
*   `ops/scripts/emergency/unpause.sh`: Unpauses all contracts.

### 2.2. Admin Key Rotation

If admin keys are compromised, they must be rotated immediately.

*   `ops/scripts/emergency/rotate-keys.sh`: Initiates the key rotation process.

### 2.3. Emergency Contact List

An up-to-date emergency contact list will be maintained in a secure location.

## 3. Recovery Procedures

### 3.1. State Migration

In the event of a contract upgrade or a critical bug that requires a new deployment, state will be migrated to the new contracts.

*   `ops/scripts/recovery/migrate-state.sh`: A script to assist in migrating state to new contracts.

### 3.2. Ownership Restoration

If state is lost or corrupted, ownership will be restored from the latest backup.

### 3.3. User Communication

Templates for user communication during a disaster scenario will be maintained.

## 4. Failover

### 4.1. Read-Only Resolver

In the event of a partial outage, a read-only resolver can be used as a fallback. This will allow users to continue resolving names, but no new registrations or updates will be possible.

### 4.2. Cached Resolution

During a total outage, cached resolution data can be used to maintain a degraded level of service.

## 5. Disaster Recovery Drills

Disaster recovery drills will be conducted quarterly on the testnet to ensure that all procedures are effective and that the team is prepared to handle a real disaster. A runbook for these drills will be maintained in this directory.
