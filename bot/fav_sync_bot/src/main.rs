mod bot;
mod cli;
mod context;
mod handler;
mod utils;

use log::error;

#[tokio::main]
async fn main() {
    if let Err(e) = bot::run().await {
        error!("Error: {:?}", e);
    }
}
