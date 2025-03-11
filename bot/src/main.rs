use log::error;

#[tokio::main]
async fn main() {
    if let Err(e) = fav_sync_bot::run().await {
        error!("Error: {:?}", e);
    }
}
