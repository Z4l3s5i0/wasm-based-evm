mod api;
mod app;
mod bootstrap;
mod cli;
mod collector;
mod config;
mod error;
mod ethereum;
mod model;
mod storage;
mod telemetry;
mod time;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = <cli::Args as clap::Parser>::parse();

    match args.command {
        cli::Command::Run { config } => app::run(config).await,
        cli::Command::CheckConfig { config } => {
            let config = config::load_config(&config)?;
            config::validate_config(&config)?;
            println!("configuration is valid");
            Ok(())
        }
    }
}
