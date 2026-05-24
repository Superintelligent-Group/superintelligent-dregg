//! Standalone DAO treasury server binary.
//!
//! Wires the `AppState` from `pyana_dao_treasury::server` plus the
//! framework's [`QueueEndpoint`] at `/queue/proposals/*` so the same
//! programmable queue is observable both directly and through the
//! application-layer gating endpoint.

use pyana_app_framework::fee_policy::FeePolicy;
use pyana_app_framework::queue_endpoint::QueueEndpoint;
use pyana_app_framework::server::{AppConfig, AppServer};

use pyana_dao_treasury::governance::{QuorumGate, Voter};
use pyana_dao_treasury::server::{AppState, router};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let config = AppConfig::from_env().with_listen("0.0.0.0:3070");

    // Default voter set for local development. Production deployments will
    // override this via a state file or a constitutional bootstrap.
    let voters = vec![
        Voter {
            id: [1; 32],
            weight: 1,
        },
        Voter {
            id: [2; 32],
            weight: 1,
        },
        Voter {
            id: [3; 32],
            weight: 1,
        },
    ];

    let app_state = AppState::new(voters);
    let app_routes = router().with_state(app_state.clone());

    // Mount a *second* view of the same queue program (read-only-ish: the
    // public queue endpoint accepts enqueue/dequeue at the storage layer,
    // but it shares the same ProgrammableQueue handle via the Arc<Mutex>).
    //
    // We re-build the endpoint using a fresh queue with the same program so
    // its hash matches the app's; the application-layer enqueue endpoint
    // (`/proposals/{id}/enqueue`) is the gated path and the canonical
    // entry point.
    //
    // REVIEW[P2]: today the QueueEndpoint takes ownership of a fresh
    // ProgrammableQueue. To share the same queue instance with the app
    // would require changing the framework's `QueueEndpoint::new` to accept
    // an `Arc<Mutex<ProgrammableQueue>>`. For now the standalone queue at
    // `/queue/proposals` is a separate instance of the same program (same
    // VK hash), useful for inspecting the program's identity but not the
    // app's live queue contents.
    let queue_view = QuorumGate::make_queue("dao-treasury-proposals-view", [0u8; 32], 1024);
    let queue_endpoint = QueueEndpoint::new(queue_view);

    AppServer::new(config)
        .service_name("pyana-dao-treasury")
        .with_name("dao-treasury", vec!["governance".into(), "treasury".into()])
        .with_health()
        .with_cors()
        .with_fee_policy(FeePolicy::computrons_only())
        .with_queue_endpoint("/queue/proposals", queue_endpoint)
        .routes(app_routes)
        .serve()
        .await
        .unwrap();
}
