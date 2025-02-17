use crate::*;

#[near]
impl Contract {
    pub fn get_snapshot(&self) -> (MerkleTreeSnapshot, VGlobalState) {
        self.tree.get_snapshot().expect("Snapshot is not available")
    }

    pub fn get_proof(&self, account_id: AccountId) -> (MerkleProof, VAccount) {
        self.tree
            .get_proof(&account_id)
            .expect("Account is not found")
    }
}
