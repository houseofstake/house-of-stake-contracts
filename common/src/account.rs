use crate::*;

#[near(serializers=[borsh, json])]
pub struct Account {
    /// The account ID of the account. Required for the security of the Merkle Tree proofs.
    pub account_id: AccountId,
}

#[near(serializers=[borsh, json])]
pub enum VAccount {
    Current(Account),
}
