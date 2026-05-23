//! Standalone pyana-subscription server binary.

use pyana_app_framework::inbox_endpoint::InboxEndpoint;
use pyana_app_framework::server::{AppConfig, AppServer};
use pyana_subscription::server::{AppState, router};

#[tokio::main]
async fn main() {
    let config = AppConfig::from_env().with_listen("0.0.0.0:3070");
    let app_state = AppState::new();
    let inbox_endpoint = InboxEndpoint::from_inbox(app_state.inbox.clone());

    let app_routes = router().with_state(app_state);

    AppServer::new(config)
        .service_name("pyana-subscription")
        .with_health()
        .with_cors()
        .with_inbox("/inbox/subscribers", inbox_endpoint)
        .with_name(
            "subscription",
            vec!["content".into(), "subscription".into()],
        )
        .routes(app_routes)
        .serve()
        .await
        .unwrap();
}
