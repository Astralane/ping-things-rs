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
RUST_LOG=info cargo build --release
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
http_rpc: "http://api.mainnet-beta.solana.com"
ws_rpc: "wss://api.mainnet-beta.solana.com"
keypair_dir: "/Users/<user>/.config/solana/id.json"
compute_unit_price: 0
compute_unit_limit: 450
verbose_log: false
tip: 1000000
```
### Configuration Options
- rpc: RPC endpoints for the Solana cluster.
  - url: endpoint for the rpc
  - rpc_type: type of endpoint can be one of: `"solanarpc", "jito", "blockxroute", "temporal", "iris", "irispaladin", "iris2", "irisb"`
  - auth: auth header or api key for sending transaction, used in "blockxroute", "jito", "iris", "irispaladin", "iris2", "irisb"
- txns_per_run: Number of transactions per run.
- txn_delay: Delay between transactions in seconds.
- runs: Number of runs to execute.
- http_rpc: RPC endpoint used for reading on chain data, eg: recent_blockhash
- ws_rpc: RPC endpoint for subscribing to transaction updates
- keypair_dir: Path to the keypair file.
- compute_unit_price: Price per compute unit.
- compute_unit_limit: Limit of compute units.
- verbose_log: Enable verbose logging.
- tip: attach tips for jito, blockxroute, temporal and iris transactions
- max_retries: `maxRetries` passed to `sendTransaction` for solanarpc and iris

### Astralane endpoints
- `iris`: JSON-RPC `sendTransaction` with the key in the `api_key` header, e.g. `https://<region>.gateway.astralane.io/iris`. The same type works for `/express`.
- `iris2`: v2 text endpoint, base64 transaction as a `text/plain` body to `<url>?api-key=<key>&method=sendTransaction`, e.g. `https://<region>.gateway.astralane.io/iris2`.
- `irisb`: v2 binary endpoint, raw serialized transaction as an `application/octet-stream` body with the same query string, e.g. `https://<region>.gateway.astralane.io/irisb`.
- `irispaladin`: JSON-RPC `sendPaladin` with the key in the `api_key` header.

```yaml
rpc:
  "jp-iris":
    url: "https://jp.gateway.astralane.io/iris"
    auth: "<ApiKey>"
    rpc_type: "iris"
  "jp-iris2":
    url: "https://jp.gateway.astralane.io/iris2"
    auth: "<ApiKey>"
    rpc_type: "iris2"
  "jp-irisb":
    url: "https://jp.gateway.astralane.io/irisb"
    auth: "<ApiKey>"
    rpc_type: "irisb"
```
