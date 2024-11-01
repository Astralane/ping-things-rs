# ping-things-rs
A small benchmarking tool for Solana transaction sends.

## What it is
`ping-things-rs` is a small benchmarking tool based on [ping-thing-client](https://github.com/Block-Logic/ping-thing-client), designed for sending 
transactions on the Solana blockchain. It helps users evaluate the performance 
and throughput of rpc providers sends by sending similar transactions to multiple
rpcs at the same time as provided in `config.yaml`

## How to Run
### Clone the repository:
```
git clone https://github.com/Astralane/ping-things-rs.git
cd ping-things-rs
```

### Build the project:
```
cargo build --release
```
### Run the benchmark:
```
./target/release/ping-things-rs
```

## How to Configure
The configuration is managed via a config.yaml file. Below is an example of what this file looks like:

```yaml
rpc :
  "solana-public" :
    url: "https://api.mainnet-beta.solana.com"
    rpc_type: "solanarpc"

txns_per_run: 2
txn_delay: 2
runs: 2
rpc_for_read: "https://api.mainnet-beta.solana.com"
keypair_dir: "/Users/<user>/.config/solana/id.json"
enable_priority_fee: true
compute_unit_price: 0
compute_unit_limit: 450
verbose_log: false
```
### Configuration Options
- rpc: RPC endpoints for the Solana cluster.
  - url: endpoint for the rpc
  - rpc_type: type of endpoint can be one of: `"solanarpc", "jito", "blockxroute"`
  - auth: auth header for for sending transaction, used in "blockxroute"
- txns_per_run: Number of transactions per run.
- txn_delay: Delay between transactions in seconds.
- runs: Number of runs to execute.
- http_rpc: RPC endpoint used for reading on chain data, eg: recent_blockhash
- ws_rpc: RPC endpoint for subscribing to transaction updates
- keypair_dir: Path to the keypair file.
- compute_unit_price: Price per compute unit.
- compute_unit_limit: Limit of compute units.
- verbose_log: Enable verbose logging.
- tip: attach tips for jito and blockxroute transactions
