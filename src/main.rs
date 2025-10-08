mod crates_client;
mod docs_client;
mod mcp_server;
mod types;

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "stdio")]
    transport: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let server = mcp_server::CratesIoMcpServer::new().await?;
    server.run(&args.transport).await?;

    Ok(())
}
