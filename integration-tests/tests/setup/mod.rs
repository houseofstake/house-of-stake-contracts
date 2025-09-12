#[allow(dead_code)]
use common::Fraction;
use common::TimestampNs;
use near_sdk::json_types::{Base58CryptoHash, U64};
use near_sdk::{CryptoHash, Gas, NearToken, Timestamp};
use near_workspaces::network::Sandbox;
use near_workspaces::operations::Function;
use near_workspaces::{Account, AccountId, Worker};
use serde_json::json;
use sha2::Digest;
use std::str::FromStr;

pub const UNLOCK_DURATION_SECONDS: u64 = 60;
pub const VOTING_DURATION_SECONDS: u64 = 60;

pub const LOCKUP_WASM_FILEPATH: &str = "../res/local/lockup_contract.wasm";
pub const VENEAR_WASM_FILEPATH: &str = "../res/local/venear_contract.wasm";
pub const VOTING_WASM_FILEPATH: &str = "../res/local/voting_contract.wasm";
pub const SANDBOX_CONTRACT_WASM_FILEPATH: &str =
    "../res/local/sandbox_staking_whitelist_contract.wasm";

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct VenearTestWorkspace {
    pub sandbox: Worker<Sandbox>,
    pub venear: Account,
    pub staking_pool_whitelist_account: Account,
    pub staking_pool: Account,
    pub lockup_deployer: Account,
    pub venear_owner: Account,
    pub guardian: Account,
    pub voting: Option<VotingTestWorkspace>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct VotingTestWorkspace {
    pub contract: Account,
    pub owner: Account,
    pub reviewer: Account,
    pub guardian: Account,
}

#[derive(Clone, Debug)]
pub struct VenearTestWorkspaceBuilder {
    pub unlock_duration_ns: u64,
    pub local_deposit: NearToken,
    pub min_lockup_deposit: NearToken,
    pub annual_growth_rate_ns: Fraction,
    pub deploy_voting: bool,
    pub voting_duration_ns: u64,
    pub max_number_of_voting_options: u8,
    pub base_proposal_fee: NearToken,
    pub vote_storage_fee: NearToken,
    pub default_quorum_percentage: u8,
}

impl Default for VenearTestWorkspaceBuilder {
    fn default() -> Self {
        Self {
            unlock_duration_ns: UNLOCK_DURATION_SECONDS * 1_000_000_000,
            local_deposit: NearToken::from_millinear(100),
            min_lockup_deposit: NearToken::from_millinear(2000),
            // 6% annual growth rate, expressed as a fraction per nanosecond
            // 6 / (100 * 365 * 24 * 60 * 60 * 10**9)
            // The denominator is set to 10**30 to avoid precision issues with large numbers.
            // So the numerator is rounded to the closest integer 1902587519025.8752
            annual_growth_rate_ns: Fraction {
                numerator: 1902587519026.into(),
                denominator: 10u128.pow(30).into(),
            },
            deploy_voting: false,
            voting_duration_ns: VOTING_DURATION_SECONDS * 1_000_000_000,
            max_number_of_voting_options: 16,
            base_proposal_fee: NearToken::from_millinear(100),
            vote_storage_fee: NearToken::from_yoctonear(125 * 10u128.pow(19)),
            default_quorum_percentage: 30, // 30% default quorum
        }
    }
}

#[allow(dead_code)]
impl VenearTestWorkspaceBuilder {
    pub async fn build(self) -> Result<VenearTestWorkspace, Box<dyn std::error::Error>> {
        let lockup_wasm = std::fs::read(LOCKUP_WASM_FILEPATH)?;
        let lockup_hash: CryptoHash = sha2::Sha256::digest(&lockup_wasm).into();
        let lockup_size = lockup_wasm.len();
        let lockup_hash = Base58CryptoHash::from(lockup_hash);

        let venear_wasm = std::fs::read(VENEAR_WASM_FILEPATH)?;
        let sandbox_wasm = std::fs::read(SANDBOX_CONTRACT_WASM_FILEPATH)?;

        let sandbox = near_workspaces::sandbox().await?;

        let staking_pool_whitelist_account = sandbox.dev_create_account().await?;
        let outcome = staking_pool_whitelist_account
            .batch(staking_pool_whitelist_account.id())
            .deploy(&sandbox_wasm)
            .call(
                Function::new("new")
                    .args_json(json!({}))
                    .gas(Gas::from_tgas(10)),
            )
            .transact()
            .await?;
        assert!(
            outcome.is_success(),
            "Failed to deploy sandbox contract for whitelist: {:#?}",
            outcome.outcomes()
        );

        // Create a staking pool account
        let staking_pool = sandbox.dev_create_account().await?;
        let outcome = staking_pool
            .batch(staking_pool.id())
            .deploy(&sandbox_wasm)
            .call(
                Function::new("new")
                    .args_json(json!({}))
                    .gas(Gas::from_tgas(10)),
            )
            .transact()
            .await?;
        assert!(
            outcome.is_success(),
            "Failed to deploy sandbox contract for staking: {:#?}",
            outcome.outcomes()
        );

        // Whitelist the staking pool account
        let outcome = staking_pool_whitelist_account
            .call(staking_pool_whitelist_account.id(), "sandbox_whitelist")
            .args_json(json!({
                "staking_pool_account_id": staking_pool.id(),
            }))
            .transact()
            .await?;
        assert!(
            outcome.is_success(),
            "Failed to whitelist staking_pool: {:#?}",
            outcome.outcomes()
        );

        let lockup_deployer = sandbox.dev_create_account().await?;
        let venear_owner = sandbox.dev_create_account().await?;
        let guardian = sandbox.dev_create_account().await?;

        let venear = sandbox.dev_create_account().await?;
        // Need a shorter name, otherwise the lockup hash will not fit into 64 bytes
        let venear = sandbox
            .create_root_account_subaccount(
                AccountId::from_str("venear").unwrap(),
                venear.secret_key().clone(),
            )
            .await?
            .unwrap();

        let args = json!({
            "config": {
                "lockup_contract_config": None::<String>,
                "unlock_duration_ns": self.unlock_duration_ns.to_string(),
                "staking_pool_whitelist_account_id": staking_pool_whitelist_account.id(),
                "lockup_code_deployers": &[lockup_deployer.id()],
                "local_deposit": self.local_deposit,
                "min_lockup_deposit": self.min_lockup_deposit,
                "owner_account_id": venear_owner.id(),
                "guardians": &[guardian.id()],
            },
            "venear_growth_config": {
                "annual_growth_rate_ns": self.annual_growth_rate_ns,
            },
        });

        let outcome = venear
            .batch(venear.id())
            .deploy(&venear_wasm)
            .call(Function::new("new").args_json(args).gas(Gas::from_tgas(10)))
            .transact()
            .await?;
        assert!(
            outcome.is_success(),
            "Failed to deploy venear: {:#?}",
            outcome.outcomes()
        );

        let storage_balance_bounds: serde_json::Value = sandbox
            .view(venear.id(), "storage_balance_bounds")
            .await?
            .json()?;

        let storage_balance_bounds_min: u128 =
            storage_balance_bounds["min"].as_str().unwrap().parse()?;
        assert_eq!(
            storage_balance_bounds_min,
            self.local_deposit.as_yoctonear(),
            "Invalid storage balance bounds"
        );

        // Adding lockup contract

        let outcome = lockup_deployer
            .call(venear.id(), "prepare_lockup_code")
            .args(lockup_wasm)
            .deposit(NearToken::from_near(2))
            .gas(Gas::from_tgas(100))
            .transact()
            .await?;

        assert!(
            outcome.is_success(),
            "Failed to add lockup code to venear: {:#?}",
            outcome.outcomes()
        );

        let contract_hash: Base58CryptoHash = outcome.unwrap().json()?;
        assert_eq!(contract_hash, lockup_hash, "Invalid contract hash");

        let outcome = venear_owner
            .call(venear.id(), "set_lockup_contract")
            .args_json(json!({
                "contract_hash": contract_hash,
                "min_lockup_deposit": self.min_lockup_deposit,
            }))
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await?;

        assert!(
            outcome.is_success(),
            "Failed to set lockup contract on venear: {:#?}",
            outcome.outcomes()
        );

        let lockup_cost: NearToken = sandbox
            .view(venear.id(), "get_lockup_deployment_cost")
            .await
            .unwrap()
            .json()
            .unwrap();

        assert_eq!(
            lockup_cost.as_yoctonear(),
            self.min_lockup_deposit.as_yoctonear(),
            "Invalid lockup cost"
        );

        let voting = if self.deploy_voting {
            let voting_wasm = std::fs::read(VOTING_WASM_FILEPATH)?;

            let contract = sandbox.dev_create_account().await?;

            let reviewer = sandbox.dev_create_account().await?;
            let owner = sandbox.dev_create_account().await?;
            let guardian = sandbox.dev_create_account().await?;

            let args = json!({
                "config": {
                    "venear_account_id": venear.id(),
                    "reviewer_ids": &[reviewer.id()],
                    "owner_account_id": owner.id(),
                    "voting_duration_ns": self.voting_duration_ns.to_string(),
                    "max_number_of_voting_options": self.max_number_of_voting_options,
                    "base_proposal_fee": self.base_proposal_fee,
                    "vote_storage_fee": self.vote_storage_fee,
                    "default_quorum_percentage": self.default_quorum_percentage,
                    "guardians": &[guardian.id()],
                },
            });

            let outcome = contract
                .batch(contract.id())
                .deploy(&voting_wasm)
                .call(Function::new("new").args_json(args).gas(Gas::from_tgas(10)))
                .transact()
                .await?;

            assert!(
                outcome.is_success(),
                "Failed to deploy voting: {:#?}",
                outcome.outcomes()
            );

            Some(VotingTestWorkspace {
                contract,
                owner,
                reviewer,
                guardian,
            })
        } else {
            None
        };

        let workspace = VenearTestWorkspace {
            sandbox,
            venear,
            staking_pool_whitelist_account,
            staking_pool,
            lockup_deployer,
            venear_owner,
            guardian,
            voting,
        };

        let config = workspace.get_config().await?;
        let lockup_config = config["lockup_contract_config"].clone();
        assert_eq!(
            lockup_config["contract_size"].as_u64().unwrap(),
            lockup_size as u64,
            "Invalid lockup contract size"
        );
        let contract_hash: Base58CryptoHash =
            serde_json::from_value(lockup_config["contract_hash"].clone()).unwrap();
        assert_eq!(contract_hash, lockup_hash, "Invalid lockup contract hash");

        Ok(workspace)
    }

    pub fn annual_growth_rate_ns(mut self, annual_growth_rate_ns: Fraction) -> Self {
        self.annual_growth_rate_ns = annual_growth_rate_ns;
        self
    }

    pub fn with_voting(mut self) -> Self {
        self.deploy_voting = true;
        self
    }
}

#[allow(dead_code)]
impl VenearTestWorkspace {
    pub async fn account_info(
        &self,
        account_id: &AccountId,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        Ok(self
            .sandbox
            .view(self.venear.id(), "get_account_info")
            .args_json(json!({ "account_id": account_id }))
            .await?
            .json()?)
    }

    pub async fn ft_balance(
        &self,
        account_id: &AccountId,
    ) -> Result<NearToken, Box<dyn std::error::Error>> {
        Ok(self
            .sandbox
            .view(self.venear.id(), "ft_balance_of")
            .args_json(json!({ "account_id": account_id }))
            .await?
            .json()?)
    }

    pub async fn create_account_with_lockup(&self) -> Result<Account, Box<dyn std::error::Error>> {
        let user_account = self.sandbox.dev_create_account().await?;

        let account_info = self.account_info(user_account.id()).await?;
        assert!(account_info.is_null(), "Account should not be registered");

        let storage_balance_bounds: serde_json::Value = self
            .sandbox
            .view(self.venear.id(), "storage_balance_bounds")
            .await?
            .json()?;

        let storage_balance_bounds_min: u128 =
            storage_balance_bounds["min"].as_str().unwrap().parse()?;

        // Attempt to register an account with less funding
        let outcome = user_account
            .call(self.venear.id(), "storage_deposit")
            .deposit(NearToken::from_yoctonear(storage_balance_bounds_min - 1))
            .args_json(json!({}))
            .transact()
            .await?;
        assert!(
            outcome.is_failure(),
            "Expected to fail on storage_deposit with less funds: {:#?}",
            outcome.outcomes()
        );

        let account_info = self.account_info(user_account.id()).await?;
        assert!(account_info.is_null(), "Account should not be registered");

        let outcome = user_account
            .call(self.venear.id(), "storage_deposit")
            .deposit(NearToken::from_yoctonear(storage_balance_bounds_min))
            .args_json(json!({}))
            .transact()
            .await?;
        assert!(
            outcome.is_success(),
            "Failed to do storage_deposit: {:#?}",
            outcome.outcomes()
        );

        let account_info = self.account_info(user_account.id()).await?;
        assert!(!account_info.is_null(), "Account should be registered");
        assert_eq!(
            account_info["account"]["account_id"].as_str().unwrap(),
            user_account.id(),
            "Invalid account id"
        );
        assert!(
            account_info["internal"]["lockup_version"].is_null(),
            "The lockup version should be null"
        );

        let lockup_cost: NearToken = self
            .sandbox
            .view(self.venear.id(), "get_lockup_deployment_cost")
            .await?
            .json()?;

        let outcome = user_account
            .call(self.venear.id(), "deploy_lockup")
            .deposit(lockup_cost)
            .args_json(json!({}))
            .gas(Gas::from_tgas(100))
            .transact()
            .await?;

        assert!(
            outcome.is_success(),
            "Failed to deploy lockup: {:#?}",
            outcome.outcomes()
        );

        let account_info = self.account_info(user_account.id()).await?;
        assert_eq!(
            account_info["internal"]["lockup_version"].as_u64().unwrap(),
            1,
            "Invalid lockup version"
        );

        Ok(user_account)
    }

    pub async fn get_lockup_account_id(
        &self,
        account_id: &AccountId,
    ) -> Result<AccountId, Box<dyn std::error::Error>> {
        Ok(self
            .sandbox
            .view(self.venear.id(), "get_lockup_account_id")
            .args_json(json!({ "account_id": account_id }))
            .await?
            .json()?)
    }

    pub async fn get_config(&self) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        Ok(self
            .sandbox
            .view(self.venear.id(), "get_config")
            .args_json(json!({}))
            .await?
            .json()?)
    }

    pub async fn get_venear_unlock_timestamp(
        &self,
        account_id: &AccountId,
    ) -> Result<TimestampNs, Box<dyn std::error::Error>> {
        Ok(self
            .sandbox
            .view(account_id, "get_venear_unlock_timestamp")
            .args_json(json!({}))
            .await?
            .json()?)
    }

    pub async fn get_venear_liquid_balance(
        &self,
        account_id: &AccountId,
    ) -> Result<NearToken, Box<dyn std::error::Error>> {
        Ok(self
            .sandbox
            .view(account_id, "get_venear_liquid_balance")
            .args_json(json!({}))
            .await?
            .json()?)
    }

    pub async fn get_venear_locked(
        &self,
        account_id: &AccountId,
    ) -> Result<NearToken, Box<dyn std::error::Error>> {
        Ok(self
            .sandbox
            .view(account_id, "get_venear_locked_balance")
            .args_json(json!({}))
            .await?
            .json()?)
    }

    pub async fn get_venear_pending(
        &self,
        account_id: &AccountId,
    ) -> Result<NearToken, Box<dyn std::error::Error>> {
        Ok(self
            .sandbox
            .view(account_id, "get_venear_pending_balance")
            .args_json(json!({}))
            .await?
            .json()?)
    }

    pub async fn get_lockup_update_nonce(
        &self,
        account_id: &AccountId,
    ) -> Result<U64, Box<dyn std::error::Error>> {
        Ok(self
            .sandbox
            .view(account_id, "get_lockup_update_nonce")
            .args_json(json!({}))
            .await?
            .json()?)
    }

    pub async fn get_proposal(
        &self,
        proposal_id: u32,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        Ok(self
            .sandbox
            .view(self.voting.as_ref().unwrap().contract.id(), "get_proposal")
            .args_json(json!({ "proposal_id": proposal_id }))
            .await?
            .json()?)
    }

    pub async fn transfer_and_lock(
        &self,
        user: &Account,
        amount: NearToken,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let lockup_id = self.get_lockup_account_id(user.id()).await?;

        let outcome = self
            .sandbox
            .root_account()
            .unwrap()
            .transfer_near(&lockup_id, amount)
            .await?;
        outcome_check(&outcome);

        user.call(&lockup_id, "lock_near")
            .args_json(json!({ "amount": amount }))
            .deposit(NearToken::from_yoctonear(1))
            .gas(Gas::from_tgas(200))
            .transact()
            .await?
            .into_result()?;
        Ok(())
    }

    pub async fn fast_forward(
        &self,
        timestamp: Timestamp,
        num_block: u64,
        max_num_iterations: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for i in 1..=max_num_iterations {
            self.sandbox.fast_forward(num_block).await?;
            let block = self.sandbox.view_block().await?;
            if block.timestamp() >= timestamp {
                break;
            } else {
                assert_ne!(i, max_num_iterations, "Unlock timestamp was not reached");
                // println!("Unlock timestamp is in the future, waiting...");
            }
        }

        Ok(())
    }

    pub fn voting_id(&self) -> &AccountId {
        self.voting.as_ref().unwrap().contract.id()
    }

    pub async fn get_proof(
        &self,
        account_id: &AccountId,
    ) -> Result<(serde_json::Value, serde_json::Value), Box<dyn std::error::Error>> {
        Ok(self
            .sandbox
            .view(self.venear.id(), "get_proof")
            .args_json(json!({ "account_id": account_id }))
            .await?
            .json()?)
    }
}

#[allow(dead_code)]
pub fn outcome_check(outcome: &near_workspaces::result::ExecutionFinalResult) {
    if outcome.failures().len() > 0 || outcome.is_failure() {
        println!("Failure outcome: {:?}", &outcome);
    }
    assert!(outcome.failures().len() == 0 && outcome.is_success());
}

#[allow(dead_code)]
pub fn assert_almost_eq(left: NearToken, right: NearToken, max_delta: NearToken) {
    let left2 = left.as_yoctonear();
    let right2 = right.as_yoctonear();
    let max_delta2 = max_delta.as_yoctonear();
    assert!(
        std::cmp::max(left2, right2) - std::cmp::min(left2, right2) <= max_delta2,
        "{}",
        format!(
            "Left {} is not even close to Right {} within delta {}",
            left.exact_amount_display(),
            right.exact_amount_display(),
            max_delta.exact_amount_display()
        )
    );
}
