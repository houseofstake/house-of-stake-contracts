use near_sdk::json_types::U64;
use near_sdk::{near, AccountId, NearToken};

#[derive(Debug, Clone)]
#[near(serializers=[borsh, json])]
pub struct Config {
    /// The account ID of the veNEAR contract.
    pub venear_account_id: AccountId,

    /// The account ID that can approve proposals.
    pub approver_id: AccountId,

    /// The account ID that can upgrade the current contract and modify the config.
    pub owner_account_id: AccountId,

    /// The maximum duration of the voting period in nanoseconds.
    pub voting_duration_ns: U64,

    /// The maximum number of voting options per proposal.
    pub max_number_of_voting_options: u16,

    /// The fee that is paid to the contract for creating a proposal.
    pub proposal_fee: NearToken,

    /// Storage fee required to store a vote for an active proposal. It can be refunded once the
    /// proposal is finalized.
    pub vote_storage_fee: NearToken,
}
