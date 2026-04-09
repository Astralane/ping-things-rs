use crate::config::RpcType;
use crate::tx_senders::transaction::{build_transaction_with_config, TransactionConfig};
use crate::tx_senders::{TxResult, TxSender};
use async_trait::async_trait;
use dashmap::DashMap;
use serde::Serialize;

// Mirrors the server-side TransactionPacket — wincode and bincode produce identical bytes
#[derive(Serialize)]
struct TransactionPacket {
    wire_transaction: Vec<u8>,
    mev_protect: bool,
    max_retry: Option<u16>,
}
use quinn::crypto::rustls::QuicClientConfig;
use quinn::{ClientConfig, Connection, Endpoint, IdleTimeout, TransportConfig};
use solana_client::rpc_client::SerializableTransaction;
use solana_keypair::Keypair;
use solana_sdk::hash::Hash;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;
use tracing::info;

pub struct IrisRsQuicTxSender {
    name: String,
    server_addr: SocketAddr,
    tx_config: TransactionConfig,
    endpoints: Arc<Vec<Endpoint>>,
    connections: Arc<DashMap<String, Connection>>,
    endpoint_counter: Arc<AtomicUsize>,
}

impl IrisRsQuicTxSender {
    pub fn new(name: String, url: String, tx_config: TransactionConfig) -> Self {
        let server_addr: SocketAddr = url
            .parse()
            .unwrap_or_else(|_| panic!("IrisRsQuic: invalid socket address '{}'", url));

        // Step 1 — TLS client config using Solana's TLS utilities
        let client_certificate =
            solana_tls_utils::QuicClientCertificate::new(Some(&Keypair::new()));
        let mut crypto = solana_tls_utils::tls_client_config_builder()
            .with_client_auth_cert(
                vec![client_certificate.certificate.clone()],
                client_certificate.key.clone_key(),
            )
            .expect("failed to set QUIC client certificates");

        // Enable 0-RTT for faster reconnections
        crypto.enable_early_data = true;
        // Solana validators identify this protocol via ALPN
        crypto.alpn_protocols = vec![b"solana-tpu".to_vec()];

        // Step 2 — Transport config
        let mut transport = TransportConfig::default();
        transport.max_idle_timeout(Some(
            IdleTimeout::try_from(Duration::from_secs(30)).unwrap(),
        ));
        transport.keep_alive_interval(Some(Duration::from_secs(1)));
        transport.send_fairness(false);

        // Step 3 — Build Quinn ClientConfig
        let quic_crypto =
            QuicClientConfig::try_from(crypto).expect("failed to build QuicClientConfig");
        let mut client_config = ClientConfig::new(Arc::new(quic_crypto));
        client_config.transport_config(Arc::new(transport));

        // Step 4 — Create QUIC endpoint bound to any available local port
        let mut endpoint =
            Endpoint::client("0.0.0.0:0".parse().unwrap()).expect("failed to create QUIC endpoint");
        endpoint.set_default_client_config(client_config);

        Self {
            name,
            server_addr,
            tx_config,
            endpoints: Arc::new(vec![endpoint]),
            connections: Arc::new(DashMap::new()),
            endpoint_counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    // Step 5 — SNI format Solana validators expect: "{ip}.{port}.sol"
    fn socket_addr_to_sni(addr: &SocketAddr) -> String {
        format!("{}.{}.sol", addr.ip(), addr.port())
    }

    // Step 6 & 7 — Get live cached connection or create a new one
    async fn get_or_connect(&self) -> anyhow::Result<Connection> {
        let key = self.server_addr.to_string();

        // Check cache for a live connection
        if let Some(conn) = self.connections.get(&key) {
            if conn.close_reason().is_none() {
                return Ok(conn.clone());
            }
        }

        // Round-robin endpoint selection
        let idx = self.endpoint_counter.fetch_add(1, Ordering::Relaxed) % self.endpoints.len();
        let endpoint = &self.endpoints[idx];
        let sni = Self::socket_addr_to_sni(&self.server_addr);

        info!("connecting to {} (sni: {})", self.server_addr, sni);

        // Attempt 0-RTT, fall back to full 1-RTT handshake
        let connection = match endpoint.connect(self.server_addr, &sni)?.into_0rtt() {
            Ok((conn, rtt_accepted)) => {
                let _ = rtt_accepted.await;
                conn
            }
            Err(connecting) => connecting.await?,
        };

        self.connections.insert(key, connection.clone());
        Ok(connection)
    }

    // Step 8 — Send data over a unidirectional stream (fire-and-forget)
    async fn send_via_quic(&self, tx_bytes: &[u8]) -> anyhow::Result<()> {
        let connection = self.get_or_connect().await?;
        let mut send_stream = connection.open_uni().await?;
        send_stream.write_all(tx_bytes).await?;
        send_stream.finish()?;
        Ok(())
    }
}

#[async_trait]
impl TxSender for IrisRsQuicTxSender {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn send_transaction(
        &self,
        index: u32,
        recent_blockhash: Hash,
    ) -> anyhow::Result<TxResult> {
        let tx = build_transaction_with_config(
            &self.tx_config,
            &RpcType::IrisRsQuic,
            index,
            recent_blockhash,
        );
        let sig = *tx.get_signature();

        let wire_transaction = bincode::serialize(&tx)
            .map_err(|e| anyhow::anyhow!("failed to serialize transaction: {}", e))?;

        let packet = TransactionPacket {
            wire_transaction,
            mev_protect: false,
            max_retry: None,
        };
        let packet_bytes = bincode::serialize(&packet)
            .map_err(|e| anyhow::anyhow!("failed to serialize TransactionPacket: {}", e))?;

        info!("sending tx via IrisRsQuic: {}", sig);

        // Step 9 — Time just the QUIC send (open stream → write → finish)
        let start = Instant::now();
        self.send_via_quic(&packet_bytes).await?;
        let send_elapsed_ms = start.elapsed().as_millis() as u64;

        info!("IrisRsQuic sent {} in {}ms", sig, send_elapsed_ms);

        Ok(TxResult::Signature(sig, send_elapsed_ms))
    }
}
