//! Standalone pyana-identity server binary.
//!
//! Uses the shared `AppServer` from `pyana-app-framework` for standard
//! middleware (health, CORS) and environment-based configuration.
//!
//! REVIEW[P1]: the new modules `inbox_delivery` and `blinded_credentials`
//! provide builder functions (`credential_inbox_endpoint`,
//! `credential_blinded_endpoint`) but neither is wired here. The running
//! `pyana-identity` binary exposes NONE of the new HTTP surface — no
//! `/inbox/credentials/*`, no `/queue/credentials/*`. The new tests in
//! `apps/identity/src/tests.rs` exercise routers in isolation only. Compare to
//! `apps/gallery/src/server.rs:158-172` which actually mounts both endpoints on
//! `AppServer`. To activate, add e.g.:
//!     .with_inbox("/inbox/credentials", credential_inbox_endpoint(256, 0))
//!     .with_blinded_endpoint("/queue/credentials", credential_blinded_endpoint(64))

use pyana_app_framework::server::{AppConfig, AppServer};
use pyana_identity::server::{AppState, router};

#[tokio::main]
async fn main() {
    let config = AppConfig::from_env().with_listen("0.0.0.0:3052");
    let app_routes = router().with_state(AppState::new());

    AppServer::new(config)
        .service_name("pyana-identity")
        .with_health()
        .with_cors()
        .routes(app_routes)
        .serve()
        .await
        .unwrap();
}
