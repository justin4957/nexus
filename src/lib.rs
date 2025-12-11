//! nexus - A channel-based terminal manager with a unified prompt interface
//!
//! This crate provides the core functionality for nexus, including:
//! - Channel management (PTY spawning, I/O handling)
//! - Client-server protocol
//! - Configuration management
//!
//! # Architecture
//!
//! nexus uses a client-server model where:
//! - The server (`nexus-server`) runs in the background managing channels
//! - The client (`nexus`) provides the user interface
//! - Communication happens over Unix domain sockets

pub mod channel;
pub mod client;
pub mod config;
pub mod protocol;
