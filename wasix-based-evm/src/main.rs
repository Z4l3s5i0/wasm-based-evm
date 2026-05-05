mod rpc;
mod storage;
mod core;
mod dev;
mod p2p;
mod frontend;
mod misc;
mod evm;
mod identity;
mod sync;

use clap::Parser;
use crate::misc::app::App;
use crate::misc::cli;

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
