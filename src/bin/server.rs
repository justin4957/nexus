//! nexus-server - Background daemon managing channels and PTYs

use anyhow::Result;
use clap::Parser;

// Re-use modules from main crate
// In a real setup, these would be in a shared library crate

#[derive(Parser)]
#[command(name = "nexus-server")]
#[command(about = "nexus background server daemon")]
struct Cli {
    /// Session name
    #[arg(short, long, default_value = "default")]
    session: String,

    /// Socket path override
    #[arg(long)]
    socket: Option<std::path::PathBuf>,

    /// Run in foreground (don't daemonize)
    #[arg(short, long)]
    foreground: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let cli = Cli::parse();

    tracing::info!("Starting nexus server for session: {}", cli.session);

    // TODO: Implement server logic
    // 1. Create Unix socket at runtime path
    // 2. Listen for client connections
    // 3. Manage channel lifecycle
    // 4. Route messages between clients and PTYs

    println!("nexus-server: not yet implemented");
    println!("Session: {}", cli.session);

    Ok(())
}
