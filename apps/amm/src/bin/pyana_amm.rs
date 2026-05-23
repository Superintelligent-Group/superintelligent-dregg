//! Standalone pyana-amm server binary.

use pyana_amm::server::{AppState, full_router};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    // full_router() mounts both the pool routes (incl. /ring/settle) AND the
    // TWAP batch queue at /queue/swaps. Using router() here would silently
    // drop the queue routes.
    let app = full_router(AppState::new());
    let listener = TcpListener::bind("0.0.0.0:3051").await.unwrap();
    eprintln!("pyana-amm listening on http://0.0.0.0:3051");
    axum::serve(listener, app).await.unwrap();
}
