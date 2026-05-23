//! Standalone pyana-orderbook server binary.
//
// REVIEW[P2]: neither the new `OrderBlindedQueue` nor `OrderbookRingParticipant`
// is wired into this binary. They exist as library types with unit tests, but
// no HTTP route exposes them and `OrderbookEngine` does not consult the
// blinded queue when submitting orders. Until wired, the framework primitives
// are effectively dead code in production. Cf. `FairDistributionEndpoint`
// (`app-framework/src/blinded_endpoint.rs`) which is the ready-made HTTP skin
// — wiring it under `/queue/orders` would match the docstring claims in
// `blinded_queue.rs`.

use pyana_orderbook::server::{AppState, router};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let app = router().with_state(AppState::new());
    let listener = TcpListener::bind("0.0.0.0:3053").await.unwrap();
    eprintln!("pyana-orderbook listening on http://0.0.0.0:3053");
    axum::serve(listener, app).await.unwrap();
}
