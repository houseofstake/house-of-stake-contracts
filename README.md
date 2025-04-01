# House-of-Stake (HoS) contracts

This repository contains the smart contracts for the House-of-Stake (HoS) project.

It contains the following contracts:

- **venear-contract**: The main contract for the HoS project, it tracks veNEAR that represents locked NEAR tokens.
- **lockup-contract**: A contract that locks NEAR tokens while being owned by the user. It's non-upgradable and doesn't
  depend on the venear-contract logic. This provides extra layer of security for the user. It allows to stake NEAR
  tokens to a validator (or towards a liquid staking as staking pools).
- **voting-contract**: A voting contract. It allows anyone to create proposals. In the current version, one of the
  reviewers has to approve the proposal, to start voting process. It uses end-of-the-block snapshots from the veNEAR
  contract to track veNEAR holders at the time of the proposal approving.

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
    deposit for the proposal and the base fee that prevents proposal spam.
  - The base fee can be changed by the owner of the voting contract.
  - A proposal has to be approved by one of the reviewers. When the proposal is approved, the latest snapshot of the
    veNEAR holders is requested from the veNEAR contract. The voting process starts after the proposal is approved or
    at the specified timestamp during the approval.
  - The duration of the voting process and the set of reviewers can be changed by the owner of the voting contract.
  - The voting process ends after the duration of the voting process. In the current version there is no action
    taken by the voting contract after the voting process ends. Only the intent is recorded in the voting contract.

### Implementation details

- veNEAR contracts implements a MerkleTree internally to store the account data. The merkle tree has a method to
  make a snapshot of the current state of the tree at the end of the previous block. It also has a method to generate
  merkle proof for the given account. Since RPC nodes return execution at the end of the block, the snapshot has
  be taken at the end of the block.
- The merkle tree is used to store the current state of the veNEAR holders. Each account stores the timestamp when
  the account was last updated, the amount of locked NEAR, the amount of extra veNEAR that is accumulated during the
  lockup period up the updated timestamp, the delegated NEAR, the delegated veNEAR, and whether this account delegates
  to someone. This information is enough to calculate the current amount of veNEAR for the account.
- The merkle tree also stores the global state, which includes the total amount of NEAR and veNEAR. During the snapshot,
  the global state is stored as well.
- When user locks NEAR in the lockup, they immediately start to receive extra veNEAR for the new total locked NEAR
  amount.
- The rate of extra veNEAR accumulation is based on the configuration of the veNEAR contract.
- When a user unlocks any amount of NEAR, the user forfeits all extra veNEAR amount accumulated during the lockup
  period.

### API

Contract methods and structures are described in
the [API](https://github.com/fastnear/house-of-stake-contracts/blob/main/API.md)
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
env CONTRACTS_SOURCE=release CHAIN_ID=testnet VOTING_DURATION_SEC=604800 scripts/deploy_all.sh $ROOT_ACCOUNT_ID
```
