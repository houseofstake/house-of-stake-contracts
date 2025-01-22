use crate::*;

#[near]
impl Contract {
    pub fn get_snapshot(&self) -> (VGlobalState, MerkleTreeSnapshot) {
        todo!()
    }

    pub fn get_proof(&self, account_id: AccountId) -> (VAccount, MerkleProof) {
        todo!()
    }
}
