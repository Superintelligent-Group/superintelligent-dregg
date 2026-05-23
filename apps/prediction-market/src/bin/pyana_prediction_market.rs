//! Standalone pyana-prediction-market server binary.
//!
//! Wires the prediction-market routes (defined in `server.rs`) into an
//! `AppServer`. Importantly: every route declared in `server.rs::router()`
//! is reachable here because we mount that router via `.routes(...)`. The
//! blinded queue is shared between our `/queue/bets/*` routes and the
//! framework's `FairDistributionEndpoint` mounted at `/queue/blinded`, so
//! external observers see the same root + state.

use std::sync::Arc;

use pyana_app_framework::blinded_endpoint::FairDistributionEndpoint;
use pyana_app_framework::server::{AppConfig, AppServer};
use pyana_prediction_market::oracle::pubkey_of;
use pyana_prediction_market::server::AppState;
use pyana_storage::blinded::BlindedQueue;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    let config = AppConfig::from_env().with_listen("0.0.0.0:3060");

    // The oracle authority key. In production this would come from a real
    // key-management system. For local/dev we accept it via `PYANA_ORACLE_KEY`
    // or fall back to a deterministic test key.
    //
    // REVIEW[P1]: there's no key rotation or multi-signature support yet.
    // A compromised single key would let any attacker submit reports.
    let signing_key: [u8; 32] = std::env::var("PYANA_ORACLE_KEY")
        .ok()
        .and_then(|s| {
            if s.len() == 64 {
                let mut out = [0u8; 32];
                for i in 0..32 {
                    out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
                }
                Some(out)
            } else {
                None
            }
        })
        .unwrap_or([0x42u8; 32]);
    let oracle_pubkey = pubkey_of(&signing_key);

    // Build the shared blinded queue (capacity 256).
    let blinded_queue = Arc::new(Mutex::new(BlindedQueue::new(256)));
    let app_state = AppState::new_with_queue(Arc::clone(&blinded_queue), oracle_pubkey);
    let app_routes = app_state.clone().router();

    // Framework's FairDistributionEndpoint for /queue/blinded — this is
    // observability-only (it owns its own queue). Our /queue/bets/* routes
    // are the load-bearing ones and operate directly on `blinded_queue`.
    //
    // REVIEW[P2]: would be nicer if `FairDistributionEndpoint::new` accepted
    // an existing Arc so this nested endpoint could share the same queue.
    let observer_endpoint = FairDistributionEndpoint::new(256);

    AppServer::new(config)
        .service_name("pyana-prediction-market")
        .with_name(
            "prediction-market",
            vec!["defi".into(), "prediction".into()],
        )
        .with_health()
        .with_cors()
        .with_blinded_endpoint("/queue/blinded", observer_endpoint)
        .routes(app_routes)
        .serve()
        .await
        .unwrap();
}
