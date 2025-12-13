//! Integration tests for the server module

use nexus::protocol::{deserialize, serialize, ClientMessage, ServerMessage, PROTOCOL_VERSION};
use nexus::server::ServerListener;
use std::os::unix::net::UnixListener as StdUnixListener;
use std::path::Path;
use std::time::Duration;
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::time::timeout;

/// Helper to read a length-prefixed message
async fn read_message(stream: &mut UnixStream) -> Option<Vec<u8>> {
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes).await.ok()?;
    let len = u32::from_be_bytes(len_bytes) as usize;
    let mut buffer = vec![0u8; len];
    stream.read_exact(&mut buffer).await.ok()?;
    Some(buffer)
}

/// Helper to write a length-prefixed message
async fn write_message(stream: &mut UnixStream, payload: &[u8]) {
    let len = payload.len() as u32;
    stream.write_all(&len.to_be_bytes()).await.unwrap();
    stream.write_all(payload).await.unwrap();
    stream.flush().await.unwrap();
}

fn can_create_unix_socket() -> bool {
    let dir = std::env::temp_dir();
    let path = dir.join("nexus_socket_test_perm.sock");
    match StdUnixListener::bind(&path) {
        Ok(listener) => {
            drop(listener);
            let _ = std::fs::remove_file(&path);
            true
        }
        Err(_) => false,
    }
}

/// Wait for the socket file to exist and connect, retrying for up to 2 seconds.
async fn wait_for_socket(path: &Path) -> UnixStream {
    let mut attempts = 0;
    loop {
        if path.exists() {
            if let Ok(stream) = UnixStream::connect(path).await {
                return stream;
            }
        }
        attempts += 1;
        if attempts > 20 {
            panic!("Timed out waiting for socket at {:?}", path);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn test_server_accepts_connection() {
    if !can_create_unix_socket() {
        eprintln!("Skipping test_server_accepts_connection: unix sockets not permitted in this environment");
        return;
    }

    let temp_dir = tempdir().unwrap();
    let socket_path = temp_dir.path().join("test.sock");

    let server = ServerListener::new("test".to_string(), socket_path.clone());
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

    // Start server in background
    let server_handle = tokio::spawn(async move { server.run(shutdown_rx).await });

    // Wait for server socket to exist and connect
    let mut stream = wait_for_socket(&socket_path).await;

    // Should receive welcome message
    let welcome_bytes = timeout(Duration::from_secs(2), read_message(&mut stream))
        .await
        .expect("Should receive message")
        .expect("Message should not be empty");

    let welcome: ServerMessage = deserialize(&welcome_bytes).expect("Should deserialize");

    match welcome {
        ServerMessage::Welcome {
            protocol_version, ..
        } => {
            assert_eq!(protocol_version, PROTOCOL_VERSION);
        }
        _ => panic!("Expected Welcome message, got {:?}", welcome),
    }

    // Clean up
    drop(stream);
    let _ = shutdown_tx.send(()).await;

    let _ = timeout(Duration::from_secs(2), server_handle).await;
}

#[tokio::test]
async fn test_server_handles_hello() {
    if !can_create_unix_socket() {
        eprintln!(
            "Skipping test_server_handles_hello: unix sockets not permitted in this environment"
        );
        return;
    }

    let temp_dir = tempdir().unwrap();
    let socket_path = temp_dir.path().join("test_hello.sock");

    let server = ServerListener::new("test_hello".to_string(), socket_path.clone());
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

    let server_handle = tokio::spawn(async move { server.run(shutdown_rx).await });

    let mut stream = wait_for_socket(&socket_path).await;

    // Read welcome
    let _ = read_message(&mut stream).await;

    // Send Hello
    let hello = ClientMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
    };
    let hello_bytes = serialize(&hello).unwrap();
    write_message(&mut stream, &hello_bytes).await;

    // Should receive Ack
    let response_bytes = timeout(Duration::from_secs(2), read_message(&mut stream))
        .await
        .expect("Should receive response")
        .expect("Response should not be empty");

    let response: ServerMessage = deserialize(&response_bytes).expect("Should deserialize");

    match response {
        ServerMessage::Ack { for_command } => {
            assert_eq!(for_command, "Hello");
        }
        _ => panic!("Expected Ack message, got {:?}", response),
    }

    drop(stream);
    let _ = shutdown_tx.send(()).await;
    let _ = timeout(Duration::from_secs(2), server_handle).await;
}

#[tokio::test]
async fn test_server_handles_list_channels() {
    if !can_create_unix_socket() {
        eprintln!("Skipping test_server_handles_list_channels: unix sockets not permitted in this environment");
        return;
    }

    let temp_dir = tempdir().unwrap();
    let socket_path = temp_dir.path().join("test_list.sock");

    let server = ServerListener::new("test_list".to_string(), socket_path.clone());
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

    let server_handle = tokio::spawn(async move { server.run(shutdown_rx).await });

    let mut stream = wait_for_socket(&socket_path).await;

    // Read welcome
    let _ = read_message(&mut stream).await;

    // Send ListChannels
    let list_msg = ClientMessage::ListChannels;
    let list_bytes = serialize(&list_msg).unwrap();
    write_message(&mut stream, &list_bytes).await;

    // Should receive ChannelList (empty for now)
    let response_bytes = timeout(Duration::from_secs(2), read_message(&mut stream))
        .await
        .expect("Should receive response")
        .expect("Response should not be empty");

    let response: ServerMessage = deserialize(&response_bytes).expect("Should deserialize");

    match response {
        ServerMessage::ChannelList { channels } => {
            assert!(channels.is_empty(), "Should have no channels yet");
        }
        _ => panic!("Expected ChannelList message, got {:?}", response),
    }

    drop(stream);
    let _ = shutdown_tx.send(()).await;
    let _ = timeout(Duration::from_secs(2), server_handle).await;
}

#[tokio::test]
async fn test_server_rejects_wrong_protocol_version() {
    if !can_create_unix_socket() {
        eprintln!("Skipping test_server_rejects_wrong_protocol_version: unix sockets not permitted in this environment");
        return;
    }

    let temp_dir = tempdir().unwrap();
    let socket_path = temp_dir.path().join("test_version.sock");

    let server = ServerListener::new("test_version".to_string(), socket_path.clone());
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

    let server_handle = tokio::spawn(async move { server.run(shutdown_rx).await });

    let mut stream = wait_for_socket(&socket_path).await;

    // Read welcome
    let _ = read_message(&mut stream).await;

    // Send Hello with wrong version
    let hello = ClientMessage::Hello {
        protocol_version: 999,
    };
    let hello_bytes = serialize(&hello).unwrap();
    write_message(&mut stream, &hello_bytes).await;

    // Should receive Error
    let response_bytes = timeout(Duration::from_secs(2), read_message(&mut stream))
        .await
        .expect("Should receive response")
        .expect("Response should not be empty");

    let response: ServerMessage = deserialize(&response_bytes).expect("Should deserialize");

    match response {
        ServerMessage::Error { message } => {
            assert!(message.contains("Protocol version mismatch"));
        }
        _ => panic!("Expected Error message, got {:?}", response),
    }

    drop(stream);
    let _ = shutdown_tx.send(()).await;
    let _ = timeout(Duration::from_secs(2), server_handle).await;
}
