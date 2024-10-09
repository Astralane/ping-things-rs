use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Deserialize)]
pub struct PingThingsArgs {
    // rpc_name -> rpc_url
    pub rpc: HashMap<String, RpcUrl>,
    pub txns_per_run: u32,
    pub txn_delay: u32,
    pub runs: u32,
    pub rpc_for_read: String,
    pub keypair_dir: String,
    pub enable_priority_fee: bool,
    pub compute_unit_price: u64,
    pub compute_unit_limit: u32,
    pub verbose_log: bool,
}
#[derive(Clone, Debug, Deserialize)]
pub struct RpcUrl {
    pub url: String,
}

impl PingThingsArgs {
    pub fn new() -> Self {
        let config_yaml = fs::read_to_string("./config.yaml").expect("cannot find config file");
        serde_yaml::from_str::<PingThingsArgs>(&config_yaml).expect("invalid config file")
    }
}

pub fn convert_to_ws(url: String) -> String {
    url.replace("http", "ws")
}
