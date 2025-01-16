# House-Of-Stake (HOS) contracts

This repository contains the smart contracts for the House-Of-Stake (HOS) project.

It contains the following contracts:

- **venear-contract**: The main contract for the HOS project, it tracks veNEAR that represents locked NEAR tokens.
- **lockup-contract**: A contract that locks NEAR tokens while being owned by the user. It's non-upgradable and doesn't
  depend on the venear-contract logic. This provides extra layer of security for the user. It allows to stake NEAR
  tokens to a validator, as well as lock liquid staked NEAR tokens.
- **voting-contract**: A contract that uses end-of-the-block snapshots from the venear-contract to allow veNEAR holders
  to vote on proposals.

## Development

### TODO:

- Lockup contract
  - [ ] Remove legacy logic for vesting schedule. There is no need to maintain vesting schedule, since there is no
    termination
    of the contract.
  - [ ] Remove legacy logic about transfer poll. It's already enabled, and doesn't need checks.
  - [ ] Add ability to lock tokens. This should notify parent venear-contract about the locked tokens. It also should
    lock tokens perpetually (until unlock is called).
  - [ ] Add ability to unlock tokens. This should notify parent venear-contract about the unlocked tokens and start
    unlock timer.
  - [ ] Add ability to lock liquid staked tokens.
  - [ ] Lockup contract should return the version of itself with every venear call.
  - [ ] A user shouldn't be able to add full-access key
  - [ ] A user should be able to nuke the contract and clean the state. This effectively is deleting the lockup contract
    and all the state associated with it. It may be needed for lockup upgrades to a new version.
  - [ ] Add unit tests
  - [ ] Add integration tests
  - [ ] Add documentation
- veNEAR contract
  - [ ] Lockup contract integration
    - [ ] Add ability to deploy lockup contract for the user
    - [ ] Add methods that receive locked near balance from the lockup contract.
    - [ ] Add methods that receive locked liquid staked near balances from the lockup contract.
  - [ ] veNEAR grows over time
  - [ ] Delegation
    - [ ] Add ability to delegate veNEAR
    - [ ] Add ability to undelegate veNEAR
    - [ ] When delegated veNEAR balance changes, it should be reflected in 2 places. The balance can't be redelgated.
  - [ ] Upgradeability
  - [ ] Add unit tests
  - [ ] Add integration tests
  - [ ] Add documentation
- Voting contract
  - [ ] Initial setup
  - [ ] Voting/proposals configuration
  - [ ] Ability to create proposals
    - [ ] Snapshot of veNEAR balances for voting
    - [ ] Control, who can create proposals
  - [ ] Voting on a proposal
    - [ ] Storage for votes
    - [ ] Time checks
    - [ ] Verification of the vote from the snapshot
    - [ ] Ability to change vote?
    - [ ] Finalization of the proposal
  - [ ] Upgradeability
  - [ ] Governance
  - [ ] Add unit tests
  - [ ] Add integration tests
  - [ ] Add documentation
