//! nexus - A channel-based terminal manager with a unified prompt interface

mod channel;
mod client;
mod config;
mod protocol;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "nexus")]
#[command(about = "A channel-based terminal manager with a unified prompt interface")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to config file
    #[arg(short, long)]
    config: Option<std::path::PathBuf>,

    /// Session name to attach to
    #[arg(short, long)]
    session: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a new session
    New {
        /// Session name
        #[arg(default_value = "default")]
        name: String,
    },
    /// Attach to an existing session
    Attach {
        /// Session name
        name: String,
    },
    /// List available sessions
    List,
    /// Kill a session
    Kill {
        /// Session name
        name: String,
    },
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

    match cli.command {
        Some(Commands::New { name }) => {
            tracing::info!("Creating new session: {}", name);
            client::start_new_session(&name).await
        }
        Some(Commands::Attach { name }) => {
            tracing::info!("Attaching to session: {}", name);
            client::attach_session(&name).await
        }
        Some(Commands::List) => client::list_sessions().await,
        Some(Commands::Kill { name }) => client::kill_session(&name).await,
        None => {
            // Default: attach to default session or create if doesn't exist
            let session_name = cli.session.unwrap_or_else(|| "default".to_string());
            client::attach_or_create(&session_name).await
        }
    }
}
