# House-Of-Stake (HOS) contracts

This repository contains the smart contracts for the House-Of-Stake (HOS) project.

It contains the following contracts:

- **venear-contract**: The main contract for the HOS project, it tracks veNEAR that represents locked NEAR tokens.
- **lockup-contract**: A contract that locks NEAR tokens while being owned by the user. It's non-upgradable and doesn't
  depend on the venear-contract logic. This provides extra layer of security for the user. It allows to stake NEAR
  tokens to a validator (or towards a liquid staking as staking pools).
- **voting-contract**: A contract that uses end-of-the-block snapshots from the venear-contract to allow veNEAR holders
  to vote on proposals.

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

### TODO:

- Lockup contract
  - [x] Remove legacy logic for vesting schedule. There is no need to maintain vesting schedule, since there is no
    termination
    of the contract.
  - [x] Remove legacy logic about transfer poll. It's already enabled, and doesn't need checks.
  - [x] Whitelist stNEAR and LINEAR as staking pools.
  - [x] Unlock timer
  - [x] Lockup contract should return the version of itself with every venear call.
  - [x] A user shouldn't be able to add full-access key
  - [x] A user should be able to nuke the contract and clean the state. This effectively is deleting the lockup contract
    and all the state associated with it. It may be needed for lockup upgrades to a new version.
    (Add a delete lockup method, requires `0` veNEAR.)
  - [x] Pass minimum storage deposit amount to lockup initialization.
  - [ ] Add JSON events
  - [x] Add unit tests
  - [ ] Add integration tests
  - [ ] Add documentation
- veNEAR contract
  - [x] Pass minimum storage deposit amount to lockup initialization.
  - [X] Ability to register account without deploying lockups.
    - [X] Reimplement as `storage_deposit` style integration.
    - [X] Lockup deployment should be optional.
  - [X] Lockup contract integration
    - [X] Add ability to deploy lockup contract for the user
    - [X] Add methods that receive locked near balance from the lockup contract.
  - [X] veNEAR grows over time
  - [X] Delegation
    - [X] Add ability to delegate veNEAR
    - [X] Add ability to undelegate veNEAR
    - [X] When delegated veNEAR balance changes, it should be reflected in 2 places. The balance can't be redelegated.
  - [ ] Add JSON events
  - [X] Configuration changes
  - [X] View methods for current lockup code
  - [X] Owner's method to update config
  - [x] Owner's method to update lockup hash
  - [x] Owner's method to update venear growth config (NOTE: not implemented, requires contract upgrade to handle
    different rates before and after change)
  - [x] Upgradeability
  - [ ] Add unit tests
  - [ ] Add integration tests
  - [ ] Add documentation
- Voting contract
  - [x] Initial setup
  - [x] Voting/proposals configuration
  - [x] Ability to create proposals
    - [x] Snapshot of veNEAR balances for voting
    - [x] Control, who can create proposals
  - [x] Voting on a proposal
    - [x] Storage for votes
    - [x] Time checks
    - [x] Verification of the vote from the snapshot
    - [x] Ability to change vote?
    - [x] Finalization of the proposal
  - [ ] Add JSON events
  - [x] Upgradeability
  - [ ] Governance
  - [ ] Add unit tests
  - [ ] Add integration tests
  - [ ] Add documentation
