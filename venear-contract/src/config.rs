use crate::*;
use common::Version;

#[derive(Clone)]
#[near(serializers=[json, borsh])]
pub struct VenearGrowsConfig {
    // TODO
}

#[derive(Clone)]
#[near(serializers=[json, borsh])]
pub struct LockupConfig {
    pub contract_size: u64,
    pub contract_version: Version,
    // TODO
}

pub struct Config {
    pub venear_growth: VenearGrowsConfig,
    pub lockup: LockupConfig,
}
