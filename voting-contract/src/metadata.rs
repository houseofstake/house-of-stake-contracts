use crate::*;

#[derive(Clone)]
#[near(serializers=[borsh])]
pub enum VProposalMetadata {
    Current(ProposalMetadata),
}

impl From<ProposalMetadata> for VProposalMetadata {
    fn from(current: ProposalMetadata) -> Self {
        Self::Current(current)
    }
}

impl From<VProposalMetadata> for ProposalMetadata {
    fn from(value: VProposalMetadata) -> Self {
        match value {
            VProposalMetadata::Current(current) => current,
        }
    }
}

/// Metadata for a proposal.
#[derive(Clone)]
#[near(serializers=[borsh, json])]
pub struct ProposalMetadata {
    /// The title of the proposal.
    pub title: Option<String>,

    /// The description of the proposal.
    pub description: Option<String>,

    /// The link to the proposal.
    pub link: Option<String>,

    /// The voting options for the proposal.
    pub voting_options: Vec<String>,

    /// Optional quorum percentage (0-100) for this proposal.
    /// If not provided, uses the default from config.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quorum_percentage: Option<u8>,
}
