mod bot;
mod cli;
mod handler;
mod storage;

use log::error;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(e) = bot::run().await {
        error!("Error: {:?}", e);
    }
}
