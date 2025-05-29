use crate::*;

#[near]
impl Contract {
    /// Returns the current snapshot of the Merkle tree and the global state.
    pub fn get_snapshot(&self) -> (MerkleTreeSnapshot, VGlobalState) {
        self.assert_not_paused();
        self.tree.get_snapshot().expect("Snapshot is not available")
    }

    /// Returns the proof for the given account and the raw account value.
    pub fn get_proof(&self, account_id: AccountId) -> (MerkleProof, VAccount) {
        self.assert_not_paused();
        self.tree
            .get_proof(&account_id)
            .expect(format!("Account {} is not found", account_id).as_str())
    }
}
