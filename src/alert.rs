use serde_json::json;
use crate::config::PingThingsArgs;

pub async fn didnt_landed_warn(config: PingThingsArgs, rpc_name : String) {
    let client = reqwest::Client::new();
    if config.discord_alert {
        let request_body = json!({
            "username": "Geyser Latency Alert",
            "embeds": [{
                "title": "txn didn't landed",
                "description": format!("Not able to land txn via RPC {}", rpc_name),
                "color": 0xf5d742
            }]
        });
        let _ = client
            .post(config.discord_webhook.clone())
            .json(&request_body)
            .send()
            .await;
    }
    if config.slack_alert {
        let request_body = json!({
            "icon_emoji": ":warning:",
            "username": "txn-sender-alert-bot",
            "text" :format!("Not able to land txn via RPC {}", rpc_name),
        });
        let _ = client
            .post(config.slack_webhook.clone())
            .json(&request_body)
            .send()
            .await;
    }
}

pub async fn connection_error_alert(config: PingThingsArgs, rpc_name: String) {
    let client = reqwest::Client::new();
    if config.discord_alert {
        let request_body = json!({
            "username": "Txn Sender Alert",
            "embeds": [{
                "title": "RPC Error",
                "description": format!("ATTENTION REQUIRED!!! \nNot able to send txn via RPC {}", rpc_name),
                "color": 0xf54242
            }]
        });
        let _ = client
            .post(config.discord_webhook.clone())
            .json(&request_body)
            .send()
            .await;
    }
    if config.slack_alert {
        let request_body = json!({
            "icon_emoji": ":rotating_light:",
            "username": "txn-sender-alert-bot",
            "text" : format!("ATTENTION REQUIRED!!! \nNot able to send txn via RPC {}", rpc_name),
        });
        let _ = client
            .post(config.slack_webhook.clone())
            .json(&request_body)
            .send()
            .await;
    }
}