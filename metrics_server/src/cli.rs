#[derive(clap::Parser, Debug)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    Run {
        #[arg(long)]
        config: std::path::PathBuf,
    },
    CheckConfig {
        #[arg(long)]
        config: std::path::PathBuf,
    },
}
