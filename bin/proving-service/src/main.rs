use crate::{commands::Cli, utils::setup_tracing};

pub(crate) mod commands;
pub(crate) mod proxy;
pub(crate) mod utils;

#[tokio::main]
async fn main() -> Result<(), String> {
    use clap::Parser;

    setup_tracing()?;

    // read command-line args
    let cli = Cli::parse();

    // execute cli action
    cli.execute().await
}
