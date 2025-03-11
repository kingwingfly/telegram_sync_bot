use tracing::{Level, error};
use tracing_subscriber::{filter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

#[tokio::main]
async fn main() {
    let filter = filter::Targets::new()
        .with_target("telegram_sync_bot", Level::INFO)
        .with_target("teloxide", Level::INFO);
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .without_time()
                .with_target(false),
        )
        .with(filter)
        .init();
    if let Err(e) = telegram_sync_bot::run().await {
        error!("Error: {:?}", e);
    }
}
