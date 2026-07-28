use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const DEFAULT_STAKING_WASM: &str = "res/local/staking_contract.wasm";
const DEFAULT_STAKING_TEST_WASM: &str = "res/local/staking_contract_test.wasm";
const DEFAULT_MOCK_POOL_WASM: &str = "res/local/mock_staking_pool_contract.wasm";

#[derive(Parser)]
#[command(
    name = "staking-cli",
    about = "Bootstrap deployment CLI for staking-contract"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Deploy a fresh staking contract or replace code without running init.
    Deploy(DeployArgs),
    /// Apply validator and catalog bootstrap configuration.
    Configure(ConfigureArgs),
    /// Run read-only post-deploy checks.
    Verify(VerifyArgs),
}

#[derive(Args)]
struct DeployArgs {
    #[command(flatten)]
    common: CommonArgs,
    /// Fresh deploy with `new(config)`.
    #[arg(long, conflicts_with = "code_only")]
    fresh: bool,
    /// Code-only deploy with `without-init-call`; never runs `migrate_state()`.
    #[arg(long)]
    code_only: bool,
    /// Contract owner for fresh deploy init. Defaults to `--account`.
    #[arg(long)]
    owner: Option<String>,
    /// WASM path. Defaults to normal or test-feature artifact based on `--test-feature`.
    #[arg(long)]
    wasm: Option<PathBuf>,
    /// Use `res/local/staking_contract_test.wasm` by default and verify test-only methods.
    #[arg(long)]
    test_feature: bool,
}

#[derive(Args)]
struct ConfigureArgs {
    #[command(flatten)]
    common: CommonArgs,
    /// Mock pool WASM used by validators with `deploy_mock_pool: true`.
    #[arg(long, default_value = DEFAULT_MOCK_POOL_WASM)]
    mock_pool_wasm: PathBuf,
}

#[derive(Args)]
struct VerifyArgs {
    #[command(flatten)]
    common: ReadOnlyCommonArgs,
    /// Also check the test-feature-only `get_block_timestamp` view.
    #[arg(long)]
    test_feature: bool,
}

#[derive(Args, Clone)]
struct CommonArgs {
    #[command(flatten)]
    readonly: ReadOnlyCommonArgs,
    /// Send transactions. Without this flag, mutating commands only print planned transactions.
    #[arg(long)]
    send: bool,
    /// Allow mutating commands on mainnet.
    #[arg(long)]
    yes_mainnet: bool,
}

#[derive(Args, Clone)]
struct ReadOnlyCommonArgs {
    /// Network passed to `near network-config`.
    #[arg(long, value_enum, default_value = "testnet")]
    network: Network,
    /// Staking contract account id.
    #[arg(long)]
    account: Option<String>,
    /// Optional bootstrap config JSON.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Signer account id. Defaults to the command-specific owner or staking account.
    #[arg(long)]
    signer: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Network {
    Testnet,
    Mainnet,
}

impl Network {
    fn as_near_network(self) -> &'static str {
        match self {
            Self::Testnet => "testnet",
            Self::Mainnet => "mainnet",
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct BootstrapConfig {
    #[serde(default)]
    staking: StakingConfig,
    #[serde(default)]
    init: InitConfig,
    #[serde(default)]
    validators: Vec<ValidatorConfig>,
    #[serde(default)]
    products: Vec<ProductConfig>,
    #[serde(default)]
    verify: VerifyConfig,
}

#[derive(Debug, Default, Deserialize)]
struct StakingConfig {
    account_id: Option<String>,
    owner_account_id: Option<String>,
    signer_account_id: Option<String>,
    wasm: Option<PathBuf>,
    test_wasm: Option<PathBuf>,
    test_feature: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct InitConfig {
    #[serde(default)]
    guardians: Vec<String>,
    #[serde(default = "default_min_lock_duration_ns")]
    min_lock_duration_ns: String,
    #[serde(default = "default_max_lock_duration_ns")]
    max_lock_duration_ns: String,
    #[serde(default = "default_epoch_unstake_settle_epochs")]
    epoch_unstake_settle_epochs: u64,
    #[serde(default = "default_min_storage_deposit")]
    min_storage_deposit: String,
    #[serde(default = "default_zero_amount")]
    per_lock_storage_stake: String,
    #[serde(default = "default_zero_amount")]
    per_farm_position_storage_stake: String,
    #[serde(default = "default_zero_amount")]
    per_purchase_storage_stake: String,
    #[serde(default = "default_min_lock_amount")]
    min_lock_amount: String,
}

impl Default for InitConfig {
    fn default() -> Self {
        Self {
            guardians: Vec::new(),
            min_lock_duration_ns: default_min_lock_duration_ns(),
            max_lock_duration_ns: default_max_lock_duration_ns(),
            epoch_unstake_settle_epochs: default_epoch_unstake_settle_epochs(),
            min_storage_deposit: default_min_storage_deposit(),
            per_lock_storage_stake: "0".to_string(),
            per_farm_position_storage_stake: "0".to_string(),
            per_purchase_storage_stake: "0".to_string(),
            min_lock_amount: default_min_lock_amount(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ValidatorConfig {
    validator_id: String,
    #[serde(default)]
    owner_account_id: Option<String>,
    #[serde(default)]
    deploy_mock_pool: bool,
}

#[derive(Debug, Deserialize)]
struct ProductConfig {
    #[serde(default)]
    product_id: Option<String>,
    validator_id: String,
    #[serde(default)]
    owner_account_id: Option<String>,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    prices: Vec<PriceConfig>,
}

#[derive(Debug, Deserialize)]
struct PriceConfig {
    #[serde(default)]
    price_id: Option<String>,
    name: String,
    #[serde(default)]
    description: String,
    amount: String,
    #[serde(default = "default_price_type")]
    price_type: String,
    #[serde(default)]
    billing_period: Option<String>,
    #[serde(default)]
    lock_factor_near_months: String,
    #[serde(default)]
    metadata: Option<Value>,
    #[serde(default)]
    set_default: bool,
}

#[derive(Debug, Deserialize)]
struct VerifyConfig {
    #[serde(default)]
    test_feature: bool,
    #[serde(default = "default_view_limit")]
    view_limit: u64,
}

impl Default for VerifyConfig {
    fn default() -> Self {
        Self {
            test_feature: false,
            view_limit: default_view_limit(),
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Deploy(args) => deploy(args),
        Commands::Configure(args) => configure(args),
        Commands::Verify(args) => verify(args),
    }
}

fn deploy(args: DeployArgs) -> Result<()> {
    let config = load_config(args.common.readonly.config.as_deref())?;
    let account_id = resolve_account(&args.common.readonly, &config)?;
    let owner = args
        .owner
        .or_else(|| config.staking.owner_account_id.clone())
        .unwrap_or_else(|| account_id.clone());
    let test_feature = args.test_feature || config.staking.test_feature.unwrap_or(false);
    let wasm = args
        .wasm
        .or_else(|| {
            if test_feature {
                config.staking.test_wasm.clone()
            } else {
                config.staking.wasm.clone()
            }
        })
        .unwrap_or_else(|| {
            if test_feature {
                DEFAULT_STAKING_TEST_WASM.into()
            } else {
                DEFAULT_STAKING_WASM.into()
            }
        });
    let mode = deploy_mode(args.fresh, args.code_only)?;
    if test_feature && args.common.readonly.network == Network::Mainnet {
        bail!("test-feature deployments are not allowed on mainnet");
    }
    if let Some(signer) = args.common.readonly.signer.as_deref() {
        if signer != account_id {
            bail!(
                "deploy signer must equal target account {account_id}; near-cli signs contract deploys with the deployed account key"
            );
        }
    }
    let ctx = MutContext::new(args.common, account_id.clone(), account_id.clone())?;
    guard_mainnet(&ctx)?;
    require_file(&wasm)?;

    println!(
        "network:        {}",
        ctx.common.readonly.network.as_near_network()
    );
    println!("account:        {account_id}");
    println!("signer:         {}", ctx.signer);
    println!("mode:           {mode}");
    println!("wasm:           {}", wasm.display());
    println!("wasm sha256:    {}", wasm_sha256(&wasm)?);
    println!("test feature:   {test_feature}");

    let mut cmd = NearCommand::new(ctx.common.readonly.network);
    cmd.arg("contract")
        .arg("deploy")
        .arg(&account_id)
        .arg("use-file")
        .arg(path_arg(&wasm));

    match mode {
        DeployMode::Fresh => {
            let init_args = json!({ "config": init_json(&owner, &config.init) });
            cmd.arg("with-init-call")
                .arg("new")
                .arg("json-args")
                .arg(init_args.to_string())
                .arg("prepaid-gas")
                .arg("100.0 Tgas")
                .arg("attached-deposit")
                .arg("0 NEAR");
        }
        DeployMode::CodeOnly => {
            cmd.arg("without-init-call");
        }
    }

    cmd.arg("network-config")
        .arg(ctx.common.readonly.network.as_near_network())
        .arg("sign-with-keychain")
        .arg("send");
    run_tx(&ctx, cmd)?;

    if ctx.common.send {
        let expected_init = match mode {
            DeployMode::Fresh => Some((owner.as_str(), &config.init)),
            DeployMode::CodeOnly => None,
        };
        verify_deployment_health(
            ctx.common.readonly.network,
            &account_id,
            expected_init,
            test_feature,
        )?;
    }
    Ok(())
}

fn configure(args: ConfigureArgs) -> Result<()> {
    let config = load_config(args.common.readonly.config.as_deref())?;
    validate_config(&config)?;
    let account_id = resolve_account(&args.common.readonly, &config)?;
    let signer = args
        .common
        .readonly
        .signer
        .clone()
        .or_else(|| config.staking.signer_account_id.clone())
        .or_else(|| config.staking.owner_account_id.clone())
        .unwrap_or_else(|| account_id.clone());
    let ctx = MutContext::new(args.common, account_id.clone(), signer)?;
    guard_mainnet(&ctx)?;

    println!("network: {}", ctx.common.readonly.network.as_near_network());
    println!("account: {account_id}");
    println!("signer:  {}", ctx.signer);
    println!("send:    {}", ctx.common.send);

    for validator in &config.validators {
        configure_validator(&ctx, &account_id, validator, &args.mock_pool_wasm)?;
    }

    let validator_owners: HashMap<String, String> = config
        .validators
        .iter()
        .filter_map(|validator| {
            non_empty(validator.owner_account_id.as_deref())
                .map(|owner| (validator.validator_id.clone(), owner.to_string()))
        })
        .collect();
    let mut product_cache = HashMap::new();
    for product in &config.products {
        let product_id = configure_product(
            &ctx,
            &account_id,
            product,
            &validator_owners,
            &mut product_cache,
        )?;
        for price in &product.prices {
            let price_id = configure_price(
                &ctx,
                &account_id,
                &product_id,
                product,
                price,
                &validator_owners,
            )?;
            if price.set_default {
                set_default_price(
                    &ctx,
                    &account_id,
                    product,
                    &product_id,
                    &price_id,
                    &validator_owners,
                )?;
            }
        }
    }

    if ctx.common.send {
        let test_feature =
            config.verify.test_feature || config.staking.test_feature.unwrap_or(false);
        verify_deployment_health(ctx.common.readonly.network, &account_id, None, test_feature)?;
        verify_configured_state(ctx.common.readonly.network, &account_id, &config)?;
    }
    Ok(())
}

fn verify(args: VerifyArgs) -> Result<()> {
    let config = load_config(args.common.config.as_deref())?;
    validate_config(&config)?;
    let account_id = resolve_account(&args.common, &config)?;
    let network = args.common.network;
    let limit = config.verify.view_limit;
    let test_feature = args.test_feature
        || config.verify.test_feature
        || config.staking.test_feature.unwrap_or(false);

    println!("network: {}", network.as_near_network());
    println!("account: {account_id}");

    let expected_owner = config
        .staking
        .owner_account_id
        .clone()
        .unwrap_or_else(|| account_id.clone());
    verify_deployment_health(
        network,
        &account_id,
        args.common
            .config
            .as_ref()
            .map(|_| (expected_owner.as_str(), &config.init)),
        test_feature,
    )?;

    if config.validators.is_empty() && config.products.is_empty() {
        let validators = view_json(
            network,
            &account_id,
            "get_validators",
            json!({ "from_index": 0, "limit": limit }),
        )?;
        println!(
            "listed validators: {}",
            validators.as_array().map_or(0, Vec::len)
        );

        let products = view_json(
            network,
            &account_id,
            "get_products",
            json!({ "from_index": 0, "limit": limit }),
        )?;
        println!(
            "listed products:   {}",
            products.as_array().map_or(0, Vec::len)
        );
    } else {
        verify_configured_state(network, &account_id, &config)?;
    }

    Ok(())
}

fn verify_deployment_health(
    network: Network,
    account_id: &str,
    expected_init: Option<(&str, &InitConfig)>,
    test_feature: bool,
) -> Result<()> {
    let version = view_json(network, account_id, "get_version", json!({}))?;
    println!("version: {version}");

    let contract_config = view_json(network, account_id, "get_config", json!({}))?;
    println!(
        "owner:   {}",
        contract_config
            .get("owner_account_id")
            .and_then(Value::as_str)
            .unwrap_or("<missing>")
    );
    if let Some((owner, init)) = expected_init {
        verify_init_config_matches(&contract_config, owner, init)?;
        println!("init config verified");
    }

    let bounds = view_json(network, account_id, "storage_balance_bounds", json!({}))?;
    println!("storage balance bounds: {bounds}");

    if test_feature {
        let ts = view_json(network, account_id, "get_block_timestamp", json!({}))?;
        println!("test clock: {ts}");
    }

    Ok(())
}

fn verify_init_config_matches(stored: &Value, owner: &str, init: &InitConfig) -> Result<()> {
    let expected = init_json(owner, init);
    let expected_fields = expected
        .as_object()
        .ok_or_else(|| anyhow!("internal error: init_json did not produce an object"))?;
    for (field, expected_value) in expected_fields {
        let actual = stored.get(field).unwrap_or(&Value::Null);
        if actual != expected_value {
            bail!("init config field {field} mismatch: expected {expected_value}, got {actual}");
        }
    }
    Ok(())
}

fn configure_validator(
    ctx: &MutContext,
    staking_account: &str,
    validator: &ValidatorConfig,
    mock_pool_wasm: &Path,
) -> Result<()> {
    let existing = view_json(
        ctx.common.readonly.network,
        staking_account,
        "get_validator",
        json!({ "validator_id": validator.validator_id }),
    )?;
    if !existing.is_null() {
        assert_active_status(&existing, &validator.validator_id)?;
    }

    if validator.deploy_mock_pool {
        require_file(mock_pool_wasm)?;
        let owner = validator
            .owner_account_id
            .as_deref()
            .unwrap_or(ctx.signer.as_str());
        if mock_pool_has_owner(ctx.common.readonly.network, &validator.validator_id, owner)? {
            println!("mock pool already deployed: {}", validator.validator_id);
        } else {
            let init_args = json!({ "owner_id": owner });
            let mut cmd = NearCommand::new(ctx.common.readonly.network);
            cmd.arg("contract")
                .arg("deploy")
                .arg(&validator.validator_id)
                .arg("use-file")
                .arg(path_arg(mock_pool_wasm))
                .arg("with-init-call")
                .arg("new")
                .arg("json-args")
                .arg(init_args.to_string())
                .arg("prepaid-gas")
                .arg("50.0 Tgas")
                .arg("attached-deposit")
                .arg("0 NEAR")
                .arg("network-config")
                .arg(ctx.common.readonly.network.as_near_network())
                .arg("sign-with-keychain")
                .arg("send");
            run_tx(ctx, cmd)?;
        }
    }

    if !existing.is_null() {
        println!("validator already exists: {}", validator.validator_id);
        return Ok(());
    }

    near_tx(
        ctx,
        staking_account,
        "add_validator",
        json!({ "validator_id": validator.validator_id }),
        "50.0 Tgas",
        "1 yoctoNEAR",
        ctx.signer.as_str(),
    )
}

fn mock_pool_has_owner(network: Network, validator_id: &str, owner: &str) -> Result<bool> {
    match view_json(network, validator_id, "get_owner_id", json!({})) {
        Ok(stored) if stored.as_str() == Some(owner) => Ok(true),
        Ok(stored) if stored.as_str().is_some() => {
            bail!(
                "mock pool {validator_id} is already deployed with owner {}, expected {owner}",
                stored
            )
        }
        Ok(stored) => bail!("mock pool {validator_id} returned unexpected owner value: {stored}"),
        Err(err) if is_missing_contract_view_error(&err) => Ok(false),
        Err(err) => Err(err).with_context(|| {
            format!("failed to inspect existing mock pool contract {validator_id}")
        }),
    }
}

fn configure_product(
    ctx: &MutContext,
    staking_account: &str,
    product: &ProductConfig,
    validator_owners: &HashMap<String, String>,
    cache: &mut HashMap<(String, String), String>,
) -> Result<String> {
    if let Some(product_id) = non_empty(product.product_id.as_deref()) {
        sync_product_by_id(ctx, staking_account, product_id, product, validator_owners)?;
        return Ok(product_id.to_string());
    }

    let key = (product.validator_id.clone(), product.name.clone());
    if let Some(product_id) = cache.get(&key) {
        return Ok(product_id.clone());
    }

    if let Some(product_id) = find_product(ctx.common.readonly.network, staking_account, product)? {
        println!("product already exists: {} ({product_id})", product.name);
        cache.insert(key, product_id.clone());
        return Ok(product_id);
    }

    let signer = catalog_signer(ctx, product, validator_owners);
    near_tx(
        ctx,
        staking_account,
        "create_product",
        json!({
            "validator_id": product.validator_id,
            "name": product.name,
            "description": product.description,
        }),
        "200.0 Tgas",
        "1 yoctoNEAR",
        signer,
    )?;

    if !ctx.common.send {
        let placeholder = format!("<product id for {}>", product.name);
        cache.insert(key, placeholder.clone());
        return Ok(placeholder);
    }

    let product_id = find_product(ctx.common.readonly.network, staking_account, product)?
        .ok_or_else(|| anyhow!("created product was not found by name: {}", product.name))?;
    cache.insert(key, product_id.clone());
    Ok(product_id)
}

fn sync_product_by_id(
    ctx: &MutContext,
    staking_account: &str,
    product_id: &str,
    product: &ProductConfig,
    validator_owners: &HashMap<String, String>,
) -> Result<()> {
    let stored = view_json(
        ctx.common.readonly.network,
        staking_account,
        "get_product",
        json!({ "product_id": product_id }),
    )?;
    if stored.is_null() {
        bail!("configured product_id was not found: {product_id}");
    }
    if stored.get("validator_id").and_then(Value::as_str) != Some(product.validator_id.as_str()) {
        bail!("configured product_id {product_id} belongs to a different validator");
    }
    assert_active_status(&stored, product_id)?;

    let current_name = stored.get("name").and_then(Value::as_str);
    let current_description = stored.get("description").and_then(Value::as_str);
    if current_name == Some(product.name.as_str())
        && current_description == Some(product.description.as_str())
    {
        println!(
            "product already up to date: {} ({product_id})",
            product.name
        );
        return Ok(());
    }

    let signer = catalog_signer(ctx, product, validator_owners);
    near_tx(
        ctx,
        staking_account,
        "edit_product",
        json!({
            "product_id": product_id,
            "name": product.name,
            "description": product.description,
        }),
        "200.0 Tgas",
        "1 yoctoNEAR",
        signer,
    )
}

fn configure_price(
    ctx: &MutContext,
    staking_account: &str,
    product_id: &str,
    product: &ProductConfig,
    price: &PriceConfig,
    validator_owners: &HashMap<String, String>,
) -> Result<String> {
    if let Some(price_id) = non_empty(price.price_id.as_deref()) {
        sync_price_by_id(
            ctx,
            staking_account,
            product_id,
            price_id,
            product,
            price,
            validator_owners,
        )?;
        return Ok(price_id.to_string());
    }

    if let Some(price_id) = find_price(
        ctx.common.readonly.network,
        staking_account,
        product_id,
        price,
    )? {
        println!("price already exists: {} ({price_id})", price.name);
        return Ok(price_id);
    }

    let signer = catalog_signer(ctx, product, validator_owners);
    near_tx(
        ctx,
        staking_account,
        "create_price",
        json!({
            "product_id": product_id,
            "name": price.name,
            "description": price.description,
            "amount": price.amount,
            "price_type": price.price_type,
            "billing_period": price.billing_period,
            "lock_factor_near_months": price.lock_factor_near_months,
            "metadata": price.metadata,
        }),
        "200.0 Tgas",
        "1 yoctoNEAR",
        signer,
    )?;

    if !ctx.common.send {
        return Ok(format!("<price id for {}>", price.name));
    }

    find_price(
        ctx.common.readonly.network,
        staking_account,
        product_id,
        price,
    )?
    .ok_or_else(|| anyhow!("created price was not found by name: {}", price.name))
}

fn sync_price_by_id(
    ctx: &MutContext,
    staking_account: &str,
    product_id: &str,
    price_id: &str,
    product: &ProductConfig,
    price: &PriceConfig,
    validator_owners: &HashMap<String, String>,
) -> Result<()> {
    let stored = view_json(
        ctx.common.readonly.network,
        staking_account,
        "get_price",
        json!({ "price_id": price_id }),
    )?;
    if stored.is_null() {
        bail!("configured price_id was not found: {price_id}");
    }
    if stored.get("product_id").and_then(Value::as_str) != Some(product_id) {
        bail!("configured price_id {price_id} belongs to a different product");
    }
    assert_active_status(&stored, price_id)?;

    assert_immutable_price_field(&stored, "amount", &price.amount, price_id)?;
    assert_immutable_price_field(&stored, "price_type", &price.price_type, price_id)?;
    assert_immutable_optional_price_field(
        &stored,
        "billing_period",
        price.billing_period.as_deref(),
        price_id,
    )?;
    assert_immutable_price_field(
        &stored,
        "lock_factor_near_months",
        &price.lock_factor_near_months,
        price_id,
    )?;

    let current_name = stored.get("name").and_then(Value::as_str);
    let current_description = stored.get("description").and_then(Value::as_str);
    let metadata_update = price_metadata_update(&stored, price.metadata.as_ref(), price_id)?;
    if current_name == Some(price.name.as_str())
        && current_description == Some(price.description.as_str())
        && metadata_matches(&stored, price.metadata.as_ref())
    {
        println!("price already up to date: {} ({price_id})", price.name);
        return Ok(());
    }

    let signer = catalog_signer(ctx, product, validator_owners);
    near_tx(
        ctx,
        staking_account,
        "edit_price",
        json!({
            "price_id": price_id,
            "name": price.name,
            "description": price.description,
            "metadata": metadata_update,
        }),
        "200.0 Tgas",
        "1 yoctoNEAR",
        signer,
    )
}

fn verify_configured_state(
    network: Network,
    staking_account: &str,
    config: &BootstrapConfig,
) -> Result<()> {
    let mut configured_price_count = 0;

    for validator in &config.validators {
        let stored = view_json(
            network,
            staking_account,
            "get_validator",
            json!({ "validator_id": validator.validator_id }),
        )?;
        if stored.is_null() {
            bail!(
                "configured validator was not found: {}",
                validator.validator_id
            );
        }
        assert_active_status(&stored, &validator.validator_id)?;
    }

    for product in &config.products {
        let product_id = if let Some(product_id) = non_empty(product.product_id.as_deref()) {
            product_id.to_string()
        } else {
            find_product(network, staking_account, product)?
                .ok_or_else(|| anyhow!("configured product was not found: {}", product.name))?
        };
        let stored_product = view_json(
            network,
            staking_account,
            "get_product",
            json!({ "product_id": product_id }),
        )?;
        verify_product_matches(&stored_product, &product_id, product)?;

        for price in &product.prices {
            configured_price_count += 1;
            let price_id = if let Some(price_id) = non_empty(price.price_id.as_deref()) {
                price_id.to_string()
            } else {
                find_price(network, staking_account, &product_id, price)?
                    .ok_or_else(|| anyhow!("configured price was not found: {}", price.name))?
            };
            let stored_price = view_json(
                network,
                staking_account,
                "get_price",
                json!({ "price_id": price_id }),
            )?;
            verify_price_matches(&stored_price, &price_id, &product_id, price)?;

            if price.set_default
                && stored_product
                    .get("default_price_id")
                    .and_then(Value::as_str)
                    != Some(price_id.as_str())
            {
                bail!(
                    "configured default price mismatch for product {product_id}: expected {price_id}, got {:?}",
                    stored_product.get("default_price_id")
                );
            }
        }
    }

    if !config.validators.is_empty() || !config.products.is_empty() || configured_price_count > 0 {
        println!(
            "configured validators verified: {}",
            config.validators.len()
        );
        println!("configured products verified:   {}", config.products.len());
        println!("configured prices verified:     {configured_price_count}");
    }

    Ok(())
}

fn verify_product_matches(stored: &Value, product_id: &str, product: &ProductConfig) -> Result<()> {
    if stored.is_null() {
        bail!("configured product_id was not found: {product_id}");
    }
    assert_string_field(stored, "validator_id", &product.validator_id, product_id)?;
    assert_string_field(stored, "name", &product.name, product_id)?;
    assert_string_field(stored, "description", &product.description, product_id)?;
    assert_active_status(stored, product_id)?;
    Ok(())
}

fn verify_price_matches(
    stored: &Value,
    price_id: &str,
    product_id: &str,
    price: &PriceConfig,
) -> Result<()> {
    if stored.is_null() {
        bail!("configured price_id was not found: {price_id}");
    }
    assert_string_field(stored, "product_id", product_id, price_id)?;
    assert_string_field(stored, "name", &price.name, price_id)?;
    assert_string_field(stored, "description", &price.description, price_id)?;
    assert_string_field(stored, "amount", &price.amount, price_id)?;
    assert_string_field(stored, "price_type", &price.price_type, price_id)?;
    assert_optional_string_field(
        stored,
        "billing_period",
        price.billing_period.as_deref(),
        price_id,
    )?;
    assert_string_field(
        stored,
        "lock_factor_near_months",
        &price.lock_factor_near_months,
        price_id,
    )?;
    assert_metadata_field(stored, price.metadata.as_ref(), price_id)?;
    assert_active_status(stored, price_id)?;
    Ok(())
}

fn assert_string_field(stored: &Value, field: &str, expected: &str, id: &str) -> Result<()> {
    let actual = stored.get(field).and_then(Value::as_str);
    if actual != Some(expected) {
        bail!("{id} field {field} mismatch: expected {expected:?}, got {actual:?}");
    }
    Ok(())
}

fn assert_optional_string_field(
    stored: &Value,
    field: &str,
    expected: Option<&str>,
    id: &str,
) -> Result<()> {
    let actual = stored.get(field).and_then(Value::as_str);
    if actual != expected {
        bail!("{id} field {field} mismatch: expected {expected:?}, got {actual:?}");
    }
    Ok(())
}

fn assert_active_status(stored: &Value, id: &str) -> Result<()> {
    assert_string_field(stored, "status", "Active", id)
}

fn assert_metadata_field(stored: &Value, expected: Option<&Value>, id: &str) -> Result<()> {
    if metadata_matches(stored, expected) {
        return Ok(());
    }
    bail!(
        "{id} field metadata mismatch: expected {:?}, got {:?}",
        expected,
        stored.get("metadata")
    )
}

fn metadata_matches(stored: &Value, expected: Option<&Value>) -> bool {
    match canonical_metadata_value(stored.get("metadata")) {
        Ok(actual) => {
            price_metadata_update(stored, expected, "<metadata>")
                .ok()
                .and_then(|value| canonical_metadata_value(Some(&value)).ok())
                .flatten()
                == actual
        }
        _ => false,
    }
}

fn price_metadata_update(
    stored: &Value,
    expected: Option<&Value>,
    price_id: &str,
) -> Result<Value> {
    let actual = canonical_metadata_value(stored.get("metadata"))?;
    let mut expected = canonical_metadata_value(expected)?;
    if actual.is_some() && expected.is_none() {
        bail!(
            "configured price_id {price_id} cannot clear metadata; edit_price treats null metadata as leave-unchanged"
        );
    }
    if let (Some(Value::Object(actual)), Some(Value::Object(expected))) =
        (actual.as_ref(), expected.as_mut())
    {
        if expected.get("farm_reward_rate") == Some(&Value::Null) {
            if let Some(reward_rate) = actual
                .get("farm_reward_rate")
                .filter(|value| !value.is_null())
                .cloned()
            {
                expected.insert("farm_reward_rate".to_string(), reward_rate);
            }
        }
    }
    Ok(expected.unwrap_or(Value::Null))
}

fn canonical_metadata_value(value: Option<&Value>) -> Result<Option<Value>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(raw)) => {
            for key in raw.keys() {
                if key != "max_amount" && key != "farm_reward_rate" {
                    bail!("unsupported price metadata field: {key}");
                }
            }
            let mut normalized = serde_json::Map::new();
            normalized.insert(
                "max_amount".to_string(),
                raw.get("max_amount").cloned().unwrap_or(Value::Null),
            );
            normalized.insert(
                "farm_reward_rate".to_string(),
                raw.get("farm_reward_rate").cloned().unwrap_or(Value::Null),
            );
            Ok(Some(Value::Object(normalized)))
        }
        Some(other) => bail!("price metadata must be null or an object, got {other}"),
    }
}

fn assert_immutable_price_field(
    stored: &Value,
    field: &str,
    expected: &str,
    price_id: &str,
) -> Result<()> {
    let actual = stored.get(field).and_then(Value::as_str);
    if actual != Some(expected) {
        bail!(
            "configured price_id {price_id} cannot update immutable field {field}: on-chain={actual:?}, config={expected:?}"
        );
    }
    Ok(())
}

fn assert_immutable_optional_price_field(
    stored: &Value,
    field: &str,
    expected: Option<&str>,
    price_id: &str,
) -> Result<()> {
    let actual = stored.get(field).and_then(Value::as_str);
    if actual != expected {
        bail!(
            "configured price_id {price_id} cannot update immutable field {field}: on-chain={actual:?}, config={expected:?}"
        );
    }
    Ok(())
}

fn set_default_price(
    ctx: &MutContext,
    staking_account: &str,
    product: &ProductConfig,
    product_id: &str,
    price_id: &str,
    validator_owners: &HashMap<String, String>,
) -> Result<()> {
    if price_id.starts_with('<') {
        println!("skip default price dry-run placeholder for product {product_id}");
        return Ok(());
    }
    let stored = view_json(
        ctx.common.readonly.network,
        staking_account,
        "get_product",
        json!({ "product_id": product_id }),
    )?;
    assert_active_status(&stored, product_id)?;
    if stored.get("default_price_id").and_then(Value::as_str) == Some(price_id) {
        println!("default price already set: {product_id} -> {price_id}");
        return Ok(());
    }
    let signer = catalog_signer(ctx, product, validator_owners);
    near_tx(
        ctx,
        staking_account,
        "set_product_default_price",
        json!({ "product_id": product_id, "price_id": price_id }),
        "200.0 Tgas",
        "1 yoctoNEAR",
        signer,
    )
}

fn catalog_signer<'a>(
    ctx: &'a MutContext,
    product: &'a ProductConfig,
    _validator_owners: &'a HashMap<String, String>,
) -> &'a str {
    if let Some(signer) = ctx.common.readonly.signer.as_deref() {
        return signer;
    }
    if let Some(owner) = non_empty(product.owner_account_id.as_deref()) {
        return owner;
    }
    ctx.signer.as_str()
}

fn find_product(
    network: Network,
    staking_account: &str,
    product: &ProductConfig,
) -> Result<Option<String>> {
    const PAGE_LIMIT: u64 = 200;

    let mut from_index = 0;
    let mut found = None;
    loop {
        let products = view_json(
            network,
            staking_account,
            "get_products",
            json!({ "from_index": from_index, "limit": PAGE_LIMIT }),
        )?;
        let Some(items) = products.as_array() else {
            bail!("get_products did not return an array");
        };
        if items.is_empty() {
            break;
        }
        for item in items {
            if item.get("validator_id").and_then(Value::as_str)
                == Some(product.validator_id.as_str())
                && item.get("name").and_then(Value::as_str) == Some(product.name.as_str())
                && item.get("status").and_then(Value::as_str) == Some("Active")
            {
                if let Some(product_id) = item.get("product_id").and_then(Value::as_str) {
                    found = Some(product_id.to_string());
                }
            }
        }
        if items.len() < PAGE_LIMIT as usize {
            break;
        }
        from_index += items.len() as u64;
    }
    Ok(found)
}

fn find_price(
    network: Network,
    staking_account: &str,
    product_id: &str,
    price: &PriceConfig,
) -> Result<Option<String>> {
    if product_id.starts_with('<') {
        return Ok(None);
    }
    let product = view_json(
        network,
        staking_account,
        "get_product",
        json!({ "product_id": product_id }),
    )?;
    let Some(price_ids) = product.get("price_ids").and_then(Value::as_array) else {
        return Ok(None);
    };
    for price_id in price_ids.iter().filter_map(Value::as_str) {
        let candidate = view_json(
            network,
            staking_account,
            "get_price",
            json!({ "price_id": price_id }),
        )?;
        if candidate.get("name").and_then(Value::as_str) == Some(price.name.as_str())
            && candidate.get("amount").and_then(Value::as_str) == Some(price.amount.as_str())
            && candidate.get("price_type").and_then(Value::as_str)
                == Some(price.price_type.as_str())
            && candidate
                .get("lock_factor_near_months")
                .and_then(Value::as_str)
                == Some(price.lock_factor_near_months.as_str())
            && candidate.get("status").and_then(Value::as_str) == Some("Active")
        {
            return Ok(Some(price_id.to_string()));
        }
    }
    Ok(None)
}

fn near_tx(
    ctx: &MutContext,
    contract_id: &str,
    method: &str,
    args: Value,
    gas: &str,
    deposit: &str,
    signer: &str,
) -> Result<()> {
    let mut cmd = NearCommand::new(ctx.common.readonly.network);
    cmd.arg("contract")
        .arg("call-function")
        .arg("as-transaction")
        .arg(contract_id)
        .arg(method)
        .arg("json-args")
        .arg(args.to_string())
        .arg("prepaid-gas")
        .arg(gas)
        .arg("attached-deposit")
        .arg(deposit)
        .arg("sign-as")
        .arg(signer)
        .arg("network-config")
        .arg(ctx.common.readonly.network.as_near_network())
        .arg("sign-with-keychain")
        .arg("send");
    run_tx(ctx, cmd)
}

fn view_json(network: Network, contract_id: &str, method: &str, args: Value) -> Result<Value> {
    let output = Command::new("near")
        .arg("--quiet")
        .arg("contract")
        .arg("call-function")
        .arg("as-read-only")
        .arg(contract_id)
        .arg(method)
        .arg("json-args")
        .arg(args.to_string())
        .arg("network-config")
        .arg(network.as_near_network())
        .arg("now")
        .output()
        .with_context(|| "failed to run near CLI")?;
    if !output.status.success() {
        bail!(
            "near view failed for {method}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_near_json(&stdout)
        .with_context(|| format!("failed to parse near output for {method}: {stdout}"))
}

fn is_missing_contract_view_error(err: &anyhow::Error) -> bool {
    let message = err.to_string().to_ascii_lowercase();
    (message.contains("account") && message.contains("does not exist"))
        || (message.contains("contract") && message.contains("does not exist"))
        || message.contains("wasm code is not deployed")
        || message.contains("code is not deployed")
        || message.contains("contract code is not deployed")
        || message.contains("codedoesnotexist")
}

fn parse_near_json(output: &str) -> Result<Value> {
    let trimmed = output.trim();
    if let Ok(value) = serde_json::from_str(trimmed) {
        return Ok(value);
    }
    for (start, ch) in trimmed.char_indices() {
        if !matches!(ch, '{' | '[' | '"' | 'n' | 't' | 'f' | '-' | '0'..='9') {
            continue;
        }
        for end in trimmed
            .char_indices()
            .map(|(idx, _)| idx)
            .chain(std::iter::once(trimmed.len()))
            .filter(|end| *end > start)
        {
            if let Ok(value) = serde_json::from_str(&trimmed[start..end]) {
                return Ok(value);
            }
        }
    }
    bail!("no JSON value found")
}

fn run_tx(ctx: &MutContext, cmd: NearCommand) -> Result<()> {
    println!("+ {}", cmd.display());
    if !ctx.common.send {
        return Ok(());
    }
    let status = Command::new("near")
        .args(cmd.args)
        .status()
        .with_context(|| "failed to run near CLI")?;
    if !status.success() {
        bail!("near transaction failed with status {status}");
    }
    Ok(())
}

struct NearCommand {
    args: Vec<OsString>,
}

impl NearCommand {
    fn new(_network: Network) -> Self {
        Self {
            args: vec![OsString::from("--quiet")],
        }
    }

    fn arg(&mut self, arg: impl Into<OsString>) -> &mut Self {
        self.args.push(arg.into());
        self
    }

    fn display(&self) -> String {
        let mut parts = vec!["near".to_string()];
        parts.extend(
            self.args
                .iter()
                .map(|arg| shell_quote(&arg.to_string_lossy())),
        );
        parts.join(" ")
    }
}

struct MutContext {
    common: CommonArgs,
    signer: String,
}

impl MutContext {
    fn new(common: CommonArgs, _account_id: String, signer: String) -> Result<Self> {
        Ok(Self { common, signer })
    }
}

#[derive(Clone, Copy)]
enum DeployMode {
    Fresh,
    CodeOnly,
}

impl std::fmt::Display for DeployMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fresh => write!(f, "fresh"),
            Self::CodeOnly => write!(f, "code-only"),
        }
    }
}

fn deploy_mode(fresh: bool, code_only: bool) -> Result<DeployMode> {
    match (fresh, code_only) {
        (true, false) => Ok(DeployMode::Fresh),
        (false, true) => Ok(DeployMode::CodeOnly),
        (false, false) => bail!("choose exactly one deploy mode: --fresh or --code-only"),
        (true, true) => bail!("--fresh and --code-only are mutually exclusive"),
    }
}

fn guard_mainnet(ctx: &MutContext) -> Result<()> {
    if ctx.common.send && ctx.common.readonly.network == Network::Mainnet && !ctx.common.yes_mainnet
    {
        bail!("mainnet mutations require --yes-mainnet");
    }
    Ok(())
}

fn load_config(path: Option<&Path>) -> Result<BootstrapConfig> {
    let Some(path) = path else {
        return Ok(BootstrapConfig::default());
    };
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read config {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse config {}", path.display()))
}

fn validate_config(config: &BootstrapConfig) -> Result<()> {
    for product in &config.products {
        let default_count = product
            .prices
            .iter()
            .filter(|price| price.set_default)
            .count();
        if default_count > 1 {
            bail!(
                "product {} config has {default_count} default prices; mark at most one price with set_default",
                product.name
            );
        }
        for price in &product.prices {
            canonical_metadata_value(price.metadata.as_ref()).with_context(|| {
                format!(
                    "invalid metadata for price {} under product {}",
                    price.name, product.name
                )
            })?;
        }
    }
    Ok(())
}

fn resolve_account(args: &ReadOnlyCommonArgs, config: &BootstrapConfig) -> Result<String> {
    args.account
        .clone()
        .or_else(|| config.staking.account_id.clone())
        .ok_or_else(|| anyhow!("missing staking account; pass --account or set staking.account_id"))
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn init_json(owner: &str, init: &InitConfig) -> Value {
    json!({
        "owner_account_id": owner,
        "proposed_new_owner_account_id": null,
        "guardians": init.guardians,
        "min_lock_duration_ns": init.min_lock_duration_ns,
        "max_lock_duration_ns": init.max_lock_duration_ns,
        "epoch_unstake_settle_epochs": init.epoch_unstake_settle_epochs,
        "min_storage_deposit": init.min_storage_deposit,
        "per_lock_storage_stake": init.per_lock_storage_stake,
        "per_farm_position_storage_stake": init.per_farm_position_storage_stake,
        "per_purchase_storage_stake": init.per_purchase_storage_stake,
        "min_lock_amount": init.min_lock_amount,
    })
}

fn wasm_sha256(path: &Path) -> Result<String> {
    let data = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(hex::encode(Sha256::digest(data)))
}

fn require_file(path: &Path) -> Result<()> {
    if !path.is_file() {
        bail!("missing file: {}", path.display());
    }
    Ok(())
}

fn path_arg(path: &Path) -> OsString {
    path.as_os_str().to_os_string()
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "-_./:=@".contains(ch))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn default_min_lock_duration_ns() -> String {
    "1".to_string()
}

fn default_max_lock_duration_ns() -> String {
    "63072000000000000".to_string()
}

fn default_epoch_unstake_settle_epochs() -> u64 {
    4
}

fn default_min_storage_deposit() -> String {
    "10000000000000000000000".to_string()
}

fn default_zero_amount() -> String {
    "0".to_string()
}

fn default_min_lock_amount() -> String {
    "1000000000000000000000000".to_string()
}

fn default_price_type() -> String {
    "OneOff".to_string()
}

fn default_view_limit() -> u64 {
    20
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_verify_section_uses_default_view_limit() {
        let config: BootstrapConfig = serde_json::from_str("{}").unwrap();

        assert!(!config.verify.test_feature);
        assert_eq!(config.verify.view_limit, default_view_limit());
    }

    #[test]
    fn init_config_verification_compares_configured_fields() {
        let mut init = InitConfig::default();
        init.guardians = vec!["guardian.testnet".to_string()];
        init.min_lock_duration_ns = "10".to_string();
        init.max_lock_duration_ns = "100".to_string();
        init.epoch_unstake_settle_epochs = 7;
        init.min_storage_deposit = "11".to_string();
        init.per_lock_storage_stake = "12".to_string();
        init.per_farm_position_storage_stake = "13".to_string();
        init.per_purchase_storage_stake = "14".to_string();
        init.min_lock_amount = "15".to_string();

        let stored = init_json("owner.testnet", &init);
        verify_init_config_matches(&stored, "owner.testnet", &init).unwrap();

        let mut mismatched = stored;
        mismatched["min_lock_amount"] = json!("16");
        let err = verify_init_config_matches(&mismatched, "owner.testnet", &init).unwrap_err();
        assert!(err.to_string().contains("min_lock_amount"));
    }

    #[test]
    fn partial_init_config_uses_zero_storage_stake_defaults() {
        let config: BootstrapConfig = serde_json::from_str(r#"{"init":{}}"#).unwrap();

        assert_eq!(config.init.per_lock_storage_stake, "0");
        assert_eq!(config.init.per_farm_position_storage_stake, "0");
        assert_eq!(config.init.per_purchase_storage_stake, "0");
    }

    #[test]
    fn metadata_matching_uses_canonical_optional_fields() {
        assert!(metadata_matches(&json!({}), None));
        assert!(metadata_matches(&json!({ "metadata": null }), None));

        let expected = json!({
            "max_amount": "100",
            "farm_reward_rate": null
        });
        assert!(metadata_matches(
            &json!({ "metadata": expected.clone() }),
            Some(&json!({ "max_amount": "100" }))
        ));
        assert!(metadata_matches(
            &json!({ "metadata": expected.clone() }),
            Some(&expected)
        ));
        assert!(!metadata_matches(
            &json!({ "metadata": { "max_amount": "101", "farm_reward_rate": null } }),
            Some(&expected)
        ));
        assert!(!metadata_matches(&json!({ "metadata": expected }), None));
    }

    #[test]
    fn metadata_validation_rejects_unknown_fields() {
        let config: BootstrapConfig = serde_json::from_str(
            r#"{
                "products": [{
                    "validator_id": "pool.testnet",
                    "name": "Product",
                    "prices": [{
                        "name": "Price",
                        "amount": "1",
                        "metadata": { "max_amunt": "100" }
                    }]
                }]
            }"#,
        )
        .unwrap();

        let err = validate_config(&config).unwrap_err();
        assert!(err.to_string().contains("invalid metadata"));
        assert!(err.chain().any(|cause| {
            cause
                .to_string()
                .contains("unsupported price metadata field")
        }));
    }

    #[test]
    fn farm_metadata_update_preserves_existing_reward_rate_when_omitted() {
        let stored = json!({
            "metadata": {
                "max_amount": "100",
                "farm_reward_rate": "5"
            }
        });
        let update = price_metadata_update(
            &stored,
            Some(&json!({
                "max_amount": "200"
            })),
            "price_1",
        )
        .unwrap();

        assert_eq!(
            update,
            json!({
                "max_amount": "200",
                "farm_reward_rate": "5"
            })
        );
        assert!(metadata_matches(
            &stored,
            Some(&json!({
                "max_amount": "100"
            }))
        ));
    }

    #[test]
    fn metadata_clear_is_rejected_for_existing_metadata() {
        let stored = json!({
            "metadata": {
                "max_amount": "100",
                "farm_reward_rate": null
            }
        });

        let err = price_metadata_update(&stored, None, "price_1").unwrap_err();
        assert!(err.to_string().contains("cannot clear metadata"));
    }

    #[test]
    fn product_and_price_verification_require_active_status() {
        let product = ProductConfig {
            product_id: None,
            validator_id: "pool.testnet".to_string(),
            owner_account_id: None,
            name: "Product".to_string(),
            description: "Description".to_string(),
            prices: Vec::new(),
        };
        let archived_product = json!({
            "validator_id": "pool.testnet",
            "name": "Product",
            "description": "Description",
            "status": "Archived"
        });
        let product_err =
            verify_product_matches(&archived_product, "prod_1", &product).unwrap_err();
        assert!(product_err.to_string().contains("status"));

        let price = PriceConfig {
            price_id: None,
            name: "Price".to_string(),
            description: "Description".to_string(),
            amount: "1".to_string(),
            price_type: "OneOff".to_string(),
            billing_period: None,
            lock_factor_near_months: "0".to_string(),
            metadata: None,
            set_default: false,
        };
        let archived_price = json!({
            "product_id": "prod_1",
            "name": "Price",
            "description": "Description",
            "amount": "1",
            "price_type": "OneOff",
            "billing_period": null,
            "lock_factor_near_months": "0",
            "metadata": null,
            "status": "Archived"
        });
        let price_err =
            verify_price_matches(&archived_price, "price_1", "prod_1", &price).unwrap_err();
        assert!(price_err.to_string().contains("status"));
    }

    #[test]
    fn missing_contract_detection_recognizes_code_does_not_exist() {
        let err = anyhow!(
            "near view failed for get_owner_id: CompilationError(CodeDoesNotExist {{ account_id: \"pool.testnet\" }})"
        );

        assert!(is_missing_contract_view_error(&err));
    }

    #[test]
    fn explicit_catalog_signer_takes_precedence() {
        let ctx = MutContext {
            common: CommonArgs {
                readonly: ReadOnlyCommonArgs {
                    network: Network::Testnet,
                    account: None,
                    config: None,
                    signer: Some("manager.testnet".to_string()),
                },
                send: false,
                yes_mainnet: false,
            },
            signer: "config-signer.testnet".to_string(),
        };
        let product = ProductConfig {
            product_id: None,
            validator_id: "pool.testnet".to_string(),
            owner_account_id: Some("pool-owner.testnet".to_string()),
            name: "Product".to_string(),
            description: String::new(),
            prices: Vec::new(),
        };

        assert_eq!(
            catalog_signer(&ctx, &product, &HashMap::new()),
            "manager.testnet"
        );
    }
}
