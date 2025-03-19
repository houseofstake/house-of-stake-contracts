#[allow(dead_code)]
use common::Fraction;
use near_sdk::json_types::Base58CryptoHash;
use near_sdk::{CryptoHash, Gas, NearToken, Timestamp};
use near_workspaces::network::Sandbox;
use near_workspaces::operations::Function;
use near_workspaces::{Account, AccountId, Worker};
use serde_json::json;
use sha2::Digest;
use std::str::FromStr;
pub const UNLOCK_DURATION_SECONDS: u64 = 60;
pub const UNLOCK_DURATION_SECONDS_PROD: u64 = 90u64 * 24 * 60 * 60;

pub const LOCKUP_WASM_FILEPATH: &str = "../res/local/lockup_contract.wasm";
pub const VENEAR_WASM_FILEPATH: &str = "../res/local/venear_contract.wasm";

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct VenearTestWorkspace {
    pub sandbox: Worker<Sandbox>,
    pub venear: Account,
    pub staking_pool_whitelist_account: Account,
    pub lockup_deployer: Account,
    pub venear_owner: Account,
}

#[derive(Clone, Debug)]
pub struct VenearTestWorkspaceBuilder {
    pub unlock_duration_ns: u64,
    pub local_deposit: NearToken,
    pub min_lockup_deposit: NearToken,
    pub annual_growth_rate_ns: Fraction,
}

impl Default for VenearTestWorkspaceBuilder {
    fn default() -> Self {
        Self {
            unlock_duration_ns: UNLOCK_DURATION_SECONDS * 1_000_000_000,
            local_deposit: NearToken::from_millinear(100),
            min_lockup_deposit: NearToken::from_millinear(1600),
            annual_growth_rate_ns: Fraction {
                numerator: 6.into(),
                denominator: (365u128 * 24 * 60 * 60 * 10u128.pow(9)).into(),
            },
        }
    }
}
impl VenearTestWorkspaceBuilder {
    pub async fn build(self) -> Result<VenearTestWorkspace, Box<dyn std::error::Error>> {
        let lockup_wasm = std::fs::read(LOCKUP_WASM_FILEPATH)?;
        let lockup_hash: CryptoHash = sha2::Sha256::digest(&lockup_wasm).into();
        let lockup_size = lockup_wasm.len();
        let lockup_hash = Base58CryptoHash::from(lockup_hash);

        let venear_wasm = std::fs::read(VENEAR_WASM_FILEPATH)?;

        let sandbox = near_workspaces::sandbox().await?;

        // TODO: Deploy
        let staking_pool_whitelist_account = sandbox.dev_create_account().await?;

        let lockup_deployer = sandbox.dev_create_account().await?;
        let venear_owner = sandbox.dev_create_account().await?;

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

        let workspace = VenearTestWorkspace {
            sandbox,
            venear,
            staking_pool_whitelist_account,
            lockup_deployer,
            venear_owner,
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
}

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

        // Attempt to register account with less funding
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
    ) -> Result<Timestamp, Box<dyn std::error::Error>> {
        Ok(self
            .sandbox
            .view(account_id, "get_venear_unlock_timestamp")
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
    ) -> Result<u64, Box<dyn std::error::Error>> {
        Ok(self
            .sandbox
            .view(account_id, "get_lockup_update_nonce")
            .args_json(json!({}))
            .await?
            .json()?)
    }
}

pub fn outcome_check(outcome: &near_workspaces::result::ExecutionFinalResult) {
    if outcome.failures().len() > 0 || outcome.is_failure() {
        println!("Failure outcome: {:?}", &outcome);
    }
    assert!(outcome.failures().len() == 0 && outcome.is_success());
}
