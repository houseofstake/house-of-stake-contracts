mod config;
mod governance;
mod metadata;
mod proposal;
mod reviewer;
mod upgrade;

use merkle_tree::{MerkleProof, MerkleTreeSnapshot};

use crate::config::Config;
use crate::metadata::VProposalMetadata;
use crate::proposal::{ProposalId, VProposal};
use common::account::*;
use common::venear::VenearGrowthConfig;
use near_sdk::store::{LookupMap, Vector};
use near_sdk::{env, near, require, AccountId, BorshStorageKey, NearToken, PanicOnDefault};

#[derive(BorshStorageKey)]
#[near]
enum StorageKeys {
    Proposal,
    ProposalMetadata,
    Votes,
    ApprovedProposals,
}

#[derive(PanicOnDefault)]
#[near(contract_state)]
pub struct Contract {
    config: Config,
    proposals: Vector<VProposal>,
    proposal_metadata: Vector<VProposalMetadata>,
    /// A map from the account ID and the proposal ID to the vote option index.
    votes: LookupMap<(AccountId, ProposalId), u32>,
    approved_proposals: Vector<ProposalId>,
}

#[near]
impl Contract {
    #[init]
    pub fn new(config: Config) -> Self {
        Self {
            config,
            proposals: Vector::new(StorageKeys::Proposal),
            proposal_metadata: Vector::new(StorageKeys::ProposalMetadata),
            votes: LookupMap::new(StorageKeys::Votes),
            approved_proposals: Vector::new(StorageKeys::ApprovedProposals),
        }
    }
}
