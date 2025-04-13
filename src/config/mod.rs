use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Clone, Deserialize)]
pub struct PingThingsArgs {
    // rpc_name -> rpc_url
    pub rpc: HashMap<String, RpcConfig>,
    pub txns_per_run: u32,
    pub txn_delay: u32,
    pub runs: u32,
    pub http_rpc: String,
    pub ws_rpc: String,
    pub keypair_dir: String,
    pub compute_unit_price: u64,
    pub compute_unit_limit: u32,
    pub tip: u64,
    pub verbose_log: bool,
    pub max_retries: u64
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "lowercase")] // Allows lowercase matching for variants
pub enum RpcType {
    #[default]
    SolanaRpc,
    BlockXRoute,
    Jito,
    Temporal,
    Iris,
    IrisPaladin
}
#[derive(Clone, Debug, Deserialize)]
pub struct RpcConfig {
    pub url: String,
    #[serde(default)]
    pub auth: Option<String>,
    #[serde(default)]
    pub rpc_type: RpcType,
}

impl PingThingsArgs {
    pub fn new() -> Self {
        let config_yaml = fs::read_to_string("./config.yaml").expect("cannot find config file");
        serde_yaml::from_str::<PingThingsArgs>(&config_yaml).expect("invalid config file")
    }
}
