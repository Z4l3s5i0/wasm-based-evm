mod error;
mod ev;
mod rpc;
mod executor;
mod storage;
mod mempool;
mod cli;
mod logging;
mod app;
mod p2p;
mod frontend;
// mod network;

use clap::Parser;
use crate::app::App;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = cli::Args::parse();

    let app = App::builder()
        .with_config(args)
        .build()
        .await?;

    app.run().await?;

    Ok(())
}
