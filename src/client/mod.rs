//! Client - user-facing terminal interface

mod input;
mod renderer;

use anyhow::Result;

/// Start a new session (spawns server if needed)
pub async fn start_new_session(name: &str) -> Result<()> {
    tracing::info!("Starting new session: {}", name);

    // TODO: Implement
    // 1. Check if server for this session already exists
    // 2. If not, spawn nexus-server in background
    // 3. Connect to server
    // 4. Enter main loop

    println!("nexus: new session '{}' - not yet implemented", name);
    Ok(())
}

/// Attach to an existing session
pub async fn attach_session(name: &str) -> Result<()> {
    tracing::info!("Attaching to session: {}", name);

    // TODO: Implement
    // 1. Find socket for session
    // 2. Connect to server
    // 3. Enter main loop

    println!("nexus: attach to '{}' - not yet implemented", name);
    Ok(())
}

/// List available sessions
pub async fn list_sessions() -> Result<()> {
    // TODO: Implement
    // 1. Scan runtime directory for session sockets
    // 2. Check which are alive
    // 3. Print list

    println!("nexus: list sessions - not yet implemented");
    println!("No sessions found.");
    Ok(())
}

/// Kill a session
pub async fn kill_session(name: &str) -> Result<()> {
    tracing::info!("Killing session: {}", name);

    // TODO: Implement
    // 1. Connect to server
    // 2. Send shutdown message
    // 3. Wait for confirmation

    println!("nexus: kill '{}' - not yet implemented", name);
    Ok(())
}

/// Attach to session or create if doesn't exist
pub async fn attach_or_create(name: &str) -> Result<()> {
    // TODO: Check if session exists first
    // For now, just try to create
    start_new_session(name).await
}
