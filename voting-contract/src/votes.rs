use crate::proposal::{Proposal, ProposalStatus, SnapshotAndState};
use crate::*;
use common::{events, near_add, near_sub};
use near_sdk::Promise;

#[near]
impl Contract {
    /// Cast a vote for the given proposal and the given voting option.
    /// The caller has to provide a merkle proof and the account state from the snapshot.
    /// The caller should match the account ID in the account state.
    /// Requires a deposit to cover the storage fee or at least 1 yoctoNEAR if changing the vote.
    #[payable]
    pub fn vote(
        &mut self,
        proposal_id: ProposalId,
        vote: u8,
        merkle_proof: MerkleProof,
        v_account: VAccount,
    ) {
        self.assert_not_paused();
        let attached_deposit = env::attached_deposit();
        require!(!attached_deposit.is_zero(), "Requires attached deposit");

        let mut proposal: Proposal = self.internal_expect_proposal_updated(proposal_id);

        match proposal.status {
            ProposalStatus::Voting => {}
            ProposalStatus::Created | ProposalStatus::Approved => {
                env::panic_str("Voting is not started yet");
            }
            ProposalStatus::Rejected => {
                env::panic_str("Proposal is rejected");
            }
            ProposalStatus::Finished => {
                env::panic_str("Voting is finished");
            }
        }

        // Validate merkle proof
        {
            let SnapshotAndState { snapshot, .. } = proposal.snapshot_and_state.as_ref().unwrap();
            require!(
                merkle_proof.is_valid(snapshot.root.into(), snapshot.length, &v_account),
                "Invalid merkle proof"
            );
        }

        let timestamp_ns = proposal.snapshot_and_state.as_ref().unwrap().timestamp_ns;
        let account: Account = v_account.into();
        let account_id = &account.account_id;
        require!(
            account_id == &env::predecessor_account_id(),
            "Account ID doesn't match the predecessor account ID"
        );
        let account_balance = account
            .venear_balance(
                timestamp_ns,
                &proposal
                    .snapshot_and_state
                    .as_ref()
                    .unwrap()
                    .venear_growth_config,
            )
            .total();
        require!(!account_balance.is_zero(), "Account has no veNEAR balance");

        let previous_vote = self.votes.get(&(account_id.clone(), proposal_id)).cloned();
        require!(
            previous_vote != Some(vote),
            "Already voted for the same option"
        );
        let mut storage_added = self.config.vote_storage_fee;
        if let Some(previous_vote) = previous_vote {
            proposal.votes[previous_vote as usize].remove_vote(account_balance);
            proposal.total_votes.remove_vote(account_balance);
            // When changing the vote. Don't need to charge the fee again.
            storage_added = NearToken::from_yoctonear(0);

            events::emit::proposal_vote_action(
                "remove_vote",
                &account_id,
                proposal_id,
                previous_vote,
                &account_balance,
            );
        }
        require!(
            (vote as usize) < proposal.votes.len(),
            "Vote option is out of bounds"
        );
        proposal.votes[vote as usize].add_vote(account_balance);
        proposal.total_votes.add_vote(account_balance);

        require!(
            attached_deposit >= storage_added,
            format!(
                "Requires deposit of {}",
                storage_added.exact_amount_display()
            )
        );

        // Note, don't refund 1 yoctoNEAR if changing the vote.
        if attached_deposit > near_add(storage_added, NearToken::from_yoctonear(1)) {
            let refund = near_sub(attached_deposit, storage_added);
            Promise::new(env::predecessor_account_id()).transfer(refund);
        }

        events::emit::proposal_vote_action(
            "add_vote",
            &account_id,
            proposal_id,
            vote,
            &account_balance,
        );

        self.votes.insert((account_id.clone(), proposal_id), vote);
        self.internal_set_proposal(proposal);
    }

    /// Returns the vote of the given account ID and proposal ID.
    pub fn get_vote(&self, account_id: AccountId, proposal_id: ProposalId) -> Option<u8> {
        self.votes.get(&(account_id, proposal_id)).cloned()
    }
}
