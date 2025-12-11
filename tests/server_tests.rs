//! Integration tests for the server module

use nexus::protocol::{deserialize, serialize, ClientMessage, ServerMessage, PROTOCOL_VERSION};
use nexus::server::ServerListener;
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

#[tokio::test]
async fn test_server_accepts_connection() {
    let temp_dir = tempdir().unwrap();
    let socket_path = temp_dir.path().join("test.sock");

    let server = ServerListener::new("test".to_string(), socket_path.clone());
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

    // Start server in background
    let server_handle = tokio::spawn(async move { server.run(shutdown_rx).await });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect to server
    let connect_result = timeout(Duration::from_secs(2), UnixStream::connect(&socket_path)).await;

    assert!(connect_result.is_ok(), "Should connect to server");
    let mut stream = connect_result.unwrap().unwrap();

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
    let temp_dir = tempdir().unwrap();
    let socket_path = temp_dir.path().join("test_hello.sock");

    let server = ServerListener::new("test_hello".to_string(), socket_path.clone());
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

    let server_handle = tokio::spawn(async move { server.run(shutdown_rx).await });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut stream = UnixStream::connect(&socket_path).await.unwrap();

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
    let temp_dir = tempdir().unwrap();
    let socket_path = temp_dir.path().join("test_list.sock");

    let server = ServerListener::new("test_list".to_string(), socket_path.clone());
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

    let server_handle = tokio::spawn(async move { server.run(shutdown_rx).await });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut stream = UnixStream::connect(&socket_path).await.unwrap();

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
    let temp_dir = tempdir().unwrap();
    let socket_path = temp_dir.path().join("test_version.sock");

    let server = ServerListener::new("test_version".to_string(), socket_path.clone());
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

    let server_handle = tokio::spawn(async move { server.run(shutdown_rx).await });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut stream = UnixStream::connect(&socket_path).await.unwrap();

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
