extern crate core;

use crate::bench::Bench;
use crate::config::PingThingsArgs;
use crate::state_listeners::ChainListener;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::info;
use crate::otpl_setup::{get_subscriber_with_otpl, init_subscriber};

mod bench;
mod config;
mod state_listeners;
mod tx_senders;
mod otpl_setup;
mod alert;

#[tokio::main]
async fn main() {
    let config = PingThingsArgs::new();
    println!("starting with config {:?}", config);

    let sub = get_subscriber_with_otpl("test".to_string(), config.rust_log.clone(), config.otpl_endpoint.clone(), std::io::stdout);
    init_subscriber(sub);
    // 
    // tracing::subscriber::set_global_default(
    //     tracing_subscriber::FmtSubscriber::builder()
    //         .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    //         .finish(),
    // )
    //     .unwrap();


    let cancellation_token = CancellationToken::new();



    // wait for end signal
    let cancellation_token_clone = cancellation_token.clone();
    tokio::spawn(async move {
        match signal::ctrl_c().await {
            Ok(_) => {
                println!("ctrl-c received, shutting down");
                cancellation_token_clone.cancel();
            }
            Err(e) => {
                println!("ctrl-c error: {:?}", e);
            }
        }
    });

    // let sub = get_subscriber_with_otpl("test".to_string(), config.rust_log.clone(), config.otpl_endpoint.clone(), std::io::stdout);
    // init_subscriber(sub);

    let chain_listener = ChainListener::new(
        config.http_rpc.clone(),
        config.ws_rpc.clone(),
        cancellation_token.clone(),
    );
    //wait for slot update and blockhash update
    while !cancellation_token.is_cancelled()
        && chain_listener
            .current_slot
            .load(core::sync::atomic::Ordering::Relaxed)
            == 0
    {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    while !cancellation_token.is_cancelled()
        && chain_listener.recent_blockhash.read().unwrap().is_none()
    {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    let bench = Bench::new(config, cancellation_token.clone());
    bench
        .start(
            chain_listener.current_slot.clone(),
            chain_listener.recent_blockhash.clone(),
        )
        .await;
    cancellation_token.cancel();
    info!("waiting for chain listener to exit");
    let _ = chain_listener.hdl.await;
    info!("exiting main");
}
