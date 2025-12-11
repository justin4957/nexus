//! nexus-server - Background daemon managing channels and PTYs

use anyhow::Result;
use clap::Parser;
use nexus::config::Config;
use nexus::server::ServerListener;
use std::path::PathBuf;
use tokio::signal;
use tokio::sync::mpsc;

#[derive(Parser)]
#[command(name = "nexus-server")]
#[command(about = "nexus background server daemon")]
#[command(version)]
struct Cli {
    /// Session name
    #[arg(short, long, default_value = "default")]
    session: String,

    /// Socket path override
    #[arg(long)]
    socket: Option<PathBuf>,

    /// Run in foreground (don't daemonize)
    #[arg(short, long)]
    foreground: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let cli = Cli::parse();

    // Load configuration
    let config = Config::load().unwrap_or_default();

    // Determine socket path
    let socket_path = cli
        .socket
        .unwrap_or_else(|| config.socket_path(&cli.session));

    tracing::info!("Starting nexus server for session: {}", cli.session);
    tracing::info!("Socket path: {:?}", socket_path);

    // Create server listener
    let server = ServerListener::new(cli.session.clone(), socket_path.clone());

    // Check if server is already running
    if server.socket_exists() {
        // Try to verify if it's a stale socket
        match tokio::net::UnixStream::connect(&socket_path).await {
            Ok(_) => {
                eprintln!(
                    "Error: Server already running for session '{}' at {:?}",
                    cli.session, socket_path
                );
                std::process::exit(1);
            }
            Err(_) => {
                // Socket exists but can't connect - it's stale, server will clean it up
            }
        }
    }

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

    // Spawn signal handlers
    let shutdown_tx_clone = shutdown_tx.clone();
    tokio::spawn(async move {
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler");
        let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())
            .expect("Failed to install SIGINT handler");

        tokio::select! {
            _ = sigterm.recv() => {
                tracing::info!("Received SIGTERM");
            }
            _ = sigint.recv() => {
                tracing::info!("Received SIGINT");
            }
        }

        let _ = shutdown_tx_clone.send(()).await;
    });

    // Run server
    if let Err(e) = server.run(shutdown_rx).await {
        tracing::error!("Server error: {}", e);
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    tracing::info!("Server shutdown complete");

    Ok(())
}
