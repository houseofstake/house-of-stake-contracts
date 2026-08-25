# House-of-Stake (HoS) contracts

This repository contains the smart contracts for the House-of-Stake (HoS) project.

It contains the following contracts:

- **venear-contract**: The main contract for the HoS project, it tracks veNEAR that represents locked NEAR tokens.
- **lockup-contract**: A contract that locks NEAR tokens while being owned by the user. It's non-upgradable and doesn't
  depend on the venear-contract logic. This provides extra layer of security for the user. It allows to stake NEAR
  tokens to a validator (or towards a liquid staking as staking pools).
- **voting-contract**: A voting contract supporting two flows. It allows anyone to create proposals. One of the
  reviewers has to approve the proposal, to start the voting process. It uses end-of-the-block snapshots from the veNEAR
  contract to track veNEAR holders at the time of the proposal approving.
  - **Classic flow**: standard approve → vote → timelock → execute lifecycle.
  - **FastTrack flow**: extends Classic with a sandbox pre-voting period, scheduled Monday voting,
    bond-based proposal fees, flexible majority types (simple/strong), proposal slashing, and council veto during voting.

## Design principles

### Security

All contracts are designed to be deployed without access keys, to make sure the contract logic can't be affected.

- **veNEAR**
  - veNEAR contract is designed to be initially controlled by the DAO owner. Initially, the owner is can be a multi-sig
    controlled by the HoS security team. Eventually, the owner should be changed to be the HoS DAO.
  - veNEAR contract implements a standard process of upgrading the contract supported by the sputnik DAO contract.
  - There should only be a one deployed version of the veNEAR contract. It will act the main entry point to the HoS
    ecosystem.
  - veNEAR acts as a factory for the lockup contracts. Only a single lockup contract can be deployed for a user. The
    lockup contract code that is being deployed can be changed by the owner of the veNEAR contract. The existing
    lockup contracts will not be affected by the change. It prevents the lockup contract from being dependent on the
    latest version of the veNEAR contract, and those contracts can't be taken over by the owner of the veNEAR.
- **lockup**
  - A lockup contract is based on the core lockup code: https://github.com/near/core-contracts/tree/master/lockup
  - The lockup contract is controlled directly by the user without veNEAR contract.
  - The lockup contracts are non-upgradable. The only way to change the lockup contract is to move the assets out of
    the old lockup contract, then issue a command to delete the lockup contract, and then deploy a new lockup contract
    through the veNEAR contract. This process guarantees that the lockup contract acts as designed by the veNEAR
    contract, at the same time, the lockup contract is not dependent on the veNEAR contract, so the funds can't be
    locked or taken over by the owner of the veNEAR contract.
  - The configuration of the lockup contract is provided by the veNEAR contract at the deployment time. There is no
    configuration changes after the deployment.
  - The lockup contract guarantees that the funds can be withdrawn after the unlocking period.
  - The unlocking period is provided by the veNEAR contract at the deployment time.
  - In order to lock NEAR, the user has to issue a command to the lockup contract. The NEAR doesn't have to be staked
    to any validator. The locked NEAR can be staked to any whitelisted staking pool or whitelisted liquid staking
    provider (e.g. STNEAR and LINEAR). The locked NEAR can be unstaked without unlocking and be staked to another
    staking pool. The user can withdraw the staking rewards from the lockup contract without unlocking the NEAR.
  - When a user locks NEAR in their lockup contract, the veNEAR contract receives an update from the lockup contract.
    The
    update includes the amount of NEAR that is locked, the version of the lockup contract and the update nonce. Each
    update should have the incremented nonce. The nonce is used to prevent delayed updates. The nonce is stored in the
    internal account data of the veNEAR contract.
  - When a lockup contract is redeployed, the nonce is reset to a larger value based on the current block height, to
    prevent delayed updates.
- **voting**
  - The voting contract is independent of the particular veNEAR contract. It can be deployed with any veNEAR
    contract. There can be more than one voting contract deployed for the same veNEAR contract.
  - The voting contract is controlled by the owner of the voting contract. The owner can be a multi-sig controlled by
    the HoS security team. Eventually, the owner should be changed to be the HoS DAO.
  - The voting contract allows anyone to create a proposal. The caller has to attach a deposit to cover the storage
    deposit for the proposal and the base fee that deters low-quality proposals.
  - The base fee can be changed by the owner of the voting contract.
  - A proposal has to be approved by one of the reviewers. When the proposal is approved, the latest snapshot of the
    veNEAR holders is requested from the veNEAR contract. The voting process starts after the proposal is approved or
    at the specified timestamp during the approval.
  - A reviewer can reject an unapproved proposal while it is in `Created`, moving it to the
    `Rejected` status.
  - Council members can veto a proposal during the timelock period (Classic) or while it is
    Scheduled/Voting (FastTrack), moving it to the `Vetoed` status.
  - On-chain quorum: proposals require a minimum participation threshold (`quorum_threshold_bps` or `quorum_floor`,
    whichever is higher) and an approval threshold to pass. The reviewer selects a simple or strong majority at
    approval time, which sets that threshold.
  - Proposals that have not been approved by a reviewer expire after a configurable deadline.
  - The duration of the voting process, the set of reviewers, the council members, the timelock duration, quorum and
    approval thresholds, and proposal expiration can be changed by the owner of the voting contract.
  - The voting process ends after the duration of the voting process. Proposals that pass voting enter a timelock
    period, during which council members can veto. After the timelock expires, signaling-only proposals (without
    actions) are finalized as `Succeeded`. Proposals with actions enter an `Executable` status, and anyone can
    trigger on-chain execution (function calls, transfers) by calling `execute_proposal`.
- **FastTrack flow**
  - Shares the same base design as Classic: independent of a particular veNEAR contract, controlled by an owner,
    and allows anyone to create proposals.
  - Key differences from Classic: bond-based proposal fees, sandbox pre-voting period, scheduled Monday voting,
    flexible majority types (simple/strong), council veto without timelock, and reviewer slash action.
  - See the proposal lifecycle below for the full status flow.

### Classic — Proposal Lifecycle

A Classic proposal moves through a series of statuses. Each transition is triggered by a specific action or condition:

1. **Created** — Anyone creates a proposal by attaching a deposit (storage + base proposal fee). No bond is required.
   - The proposal sits in `Created` waiting for a reviewer to act.
   - If no reviewer acts before `classic_proposal_expiration_ns`, the proposal becomes `Expired`.
   - If a reviewer rejects the proposal, it becomes `Rejected`.

2. **Voting** — A reviewer approves the proposal, choosing a majority type (simple or strong).
   - A veNEAR snapshot is fetched at approval time to determine voting power.
   - The chosen majority type sets the proposal's approval threshold.
   - Up to `max_active_proposals` proposals can occupy active slots (Sandbox / Scheduled /
     Voting / Timelock) simultaneously. If all slots are full, the proposal lands in
     `Queued` and is promoted FIFO as slots free up.
   - Voting begins immediately and runs for `classic_voting_duration_ns`. All vote types are allowed: "For", "Against", "Abstain".
   - Users vote using their veNEAR balance from the snapshot taken at approval time.
   - Voters can change their vote during this period.
   - When the voting duration expires, the result is evaluated:
     - **Quorum check**: total votes must meet the quorum threshold (% of supply) or the quorum floor (absolute minimum).
     - **Approval check**: "For" / ("For" + "Against") must meet the proposal's approval threshold. "Abstain" votes count toward quorum but not toward approval.
   - If either check fails, the proposal becomes `Defeated`. Otherwise it enters `Timelock`.

3. **Timelock** — Post-voting waiting period of `timelock_duration_ns` during which the proposal cannot yet execute.
   - A council member may call `veto_proposal` to move the proposal to `Vetoed`.
   - A council member may call `noveto_proposal` to waive the veto right and end the timelock immediately.
   - When the timelock expires (or is waived via `noveto_proposal`):
     - Signaling-only proposals (no actions) finalize as `Succeeded`.
     - Proposals with on-chain actions enter `Executable`.

4. **Succeeded** / **Defeated** — Terminal voting outcomes.
   - **Succeeded**: quorum and approval thresholds both met, timelock cleared, no actions to run (or all actions completed successfully).
   - **Defeated**: quorum or approval threshold not met.

5. **Executable** → **InProgress** → **Succeeded** / **Failed** — For proposals with on-chain actions.
   - **Executable**: voting succeeded, timelock cleared, and actions are ready. Anyone can call `execute_proposal`.
   - **InProgress**: actions have been dispatched, awaiting callback results.
   - **Succeeded**: all actions completed successfully.
   - **Failed**: one or more actions failed on-chain.

6. **Rejected** — A reviewer rejects a proposal in `Created` status.

7. **Vetoed** — A council member vetoes the proposal during `Timelock`.

8. **Expired** — No reviewer acted before `classic_proposal_expiration_ns` elapsed.

### FastTrack — Proposal Lifecycle

A FastTrack proposal moves through a series of statuses. Each transition is triggered by a specific action or condition:

1. **Created** — Anyone creates a proposal by attaching a deposit (storage + base proposal fee + bond).
   - The proposal sits in `Created` waiting for a reviewer to act.
   - If no reviewer acts before the expiration period, the proposal becomes `Expired` and the bond is claimable via `claim_bond`.
   - If a reviewer rejects the proposal, the bond is also claimable.

2. **Sandbox** — A reviewer approves the proposal, choosing a majority type (simple or strong).
   - The bond is forwarded to the configured `treasury_account_id` upon approval (no longer claimable by the proposer).
   - A veNEAR snapshot is fetched to determine voting power.
   - Only "For" votes are allowed during this period — no "Against" or "Abstain".
   - If "For" votes reach the sandbox threshold (e.g. 30% of total veNEAR), the proposal graduates to `Scheduled`.
   - If the sandbox duration expires without reaching the threshold, the proposal becomes `Defeated`.

3. **Scheduled** — The proposal is queued to start full voting on the next Monday (00:00 CET).
   - Up to `max_active_proposals` proposals can occupy active slots (Sandbox / Scheduled /
     Voting / Timelock) simultaneously. If all slots are full, additional approved proposals
     park in the FIFO pending queue with status `Queued` and are promoted as slots free up.
   - No new votes are cast during this waiting period.

4. **Voting** — Full voting begins on Monday. All vote types are allowed: "For", "Against", "Abstain".
   - Users vote using their veNEAR balance from the snapshot taken at approval time.
   - Voters can change their vote during this period.
   - When the voting duration expires, the result is evaluated:
     - **Quorum check**: total votes must meet the quorum threshold (% of supply) or the quorum floor (absolute minimum).
     - **Approval check**: "For" / ("For" + "Against") must meet the majority threshold. "Abstain" votes count toward quorum but not toward approval.

5. **Succeeded** / **Defeated** — Terminal voting outcomes.
   - **Succeeded**: quorum and approval thresholds both met. If the proposal has no actions, it stays here (signaling-only).
   - **Defeated**: quorum or approval threshold not met.

6. **Executable** → **InProgress** → **Succeeded** / **Failed** — For proposals with on-chain actions.
   - **Executable**: voting succeeded and actions are ready. Anyone can call `execute_proposal`.
   - **InProgress**: actions have been dispatched, awaiting callback results.
   - **Succeeded**: all actions completed successfully.
   - **Failed**: one or more actions failed on-chain.

7. **Slashed** — A reviewer marks a `Created` proposal as malicious/spam.
   - The proposer's bond is forfeited to the `treasury_account_id`. Not claimable.

8. **Rejected** — A reviewer rejects a proposal in `Created` status.
   - The bond remains claimable by the proposer via `claim_bond`.

9. **Vetoed** — A council member vetoes the proposal during `Scheduled` or `Voting`.
   - The bond was already forwarded to the treasury at approval time and is not claimable.

### Implementation details

- veNEAR contracts implements a MerkleTree internally to store the account data. The merkle tree has a method to
  make a snapshot of the current state of the tree at the end of the previous block. It also has a method to generate
  merkle proof for the given account. Since RPC nodes return execution at the end of the block, the snapshot has
  be taken at the end of the block.
- The merkle tree is used to store the current state of the veNEAR holders. Each account stores the timestamp when
  the account was last updated, the amount of locked NEAR, the amount of extra veNEAR accumulated during the lockup
  period up to the updated timestamp, the pooled NEAR and veNEAR delegated to this account by others, and the
  account's outgoing partial delegations (a list of `(delegate, bps)` entries; the undelegated remainder implicitly
  stays with the owner). This information is enough to calculate the current amount of veNEAR for the account.
- The merkle tree also stores the global state, which includes the total amount of NEAR and veNEAR. During the snapshot,
  the global state is stored as well.
- When user locks NEAR in the lockup, they immediately start to receive extra veNEAR for the new total locked NEAR
  amount.
- The rate of extra veNEAR accumulation is based on the configuration of the veNEAR contract.
- When a user unlocks any amount of NEAR, the user forfeits all extra veNEAR amount accumulated during the lockup
  period.

### API

Contract methods and structures are described in
the [API](https://github.com/houseofstake/house-of-stake-contracts/blob/main/API.md)
section.

### Events

Documentation for events is TBD

## Development

### Building

To build all the contracts locally, run the following command:

```bash
./build_all.sh
```

### Testing

To test all the contracts locally, run the following command (note, it will build the contracts first):

```bash
./test_all.sh
```

### Run end-to-end flow on testnet

```bash
./build_all.sh
scripts/test_all.sh
```

### Deploying on testnet

```bash
export ROOT_ACCOUNT_ID=hos01.testnet
env CONTRACTS_SOURCE=release CHAIN_ID=testnet CLASSIC_VOTING_DURATION_SEC=604800 scripts/deploy_all.sh $ROOT_ACCOUNT_ID
```

### Building release candidate

Check the release tags for [latest](https://github.com/houseofstake/house-of-stake-contracts/releases/tag/0.0.4)

Before building or verifying code hashes run:
```
git fetch --tags
git checkout v0.0.4
./build_release.sh
```
