//! Integration tests for protocol serialization

use nexus::protocol::{
    check_version_compatibility, deserialize, frame_message, serialize, serialize_and_frame,
    unframe_and_deserialize, unframe_message, ClientMessage, ServerMessage, MAX_MESSAGE_SIZE,
    PROTOCOL_VERSION,
};

#[test]
fn test_client_message_roundtrip() {
    let messages = vec![
        ClientMessage::Hello {
            protocol_version: 1,
        },
        ClientMessage::Input {
            data: b"hello".to_vec(),
        },
        ClientMessage::CreateChannel {
            name: "test".to_string(),
            command: Some("bash".to_string()),
            working_dir: None,
        },
        ClientMessage::SwitchChannel {
            name: "test".to_string(),
        },
        ClientMessage::ListChannels,
    ];

    for msg in messages {
        let encoded = serialize(&msg).expect("serialize failed");
        let decoded: ClientMessage = deserialize(&encoded).expect("deserialize failed");

        // Compare debug representations since ClientMessage doesn't derive PartialEq
        assert_eq!(format!("{:?}", msg), format!("{:?}", decoded));
    }
}

#[test]
fn test_server_message_roundtrip() {
    let msg = ServerMessage::Output {
        channel: "test".to_string(),
        data: b"output data".to_vec(),
        timestamp: 1234567890,
    };

    let encoded = serialize(&msg).expect("serialize failed");
    let decoded: ServerMessage = deserialize(&encoded).expect("deserialize failed");

    assert_eq!(format!("{:?}", msg), format!("{:?}", decoded));
}

#[test]
fn test_acceptance_criteria() {
    // This is the exact acceptance criteria from the issue
    let msg = ClientMessage::Input {
        data: b"test".to_vec(),
    };
    let bytes = serialize(&msg).expect("serialize failed");
    let decoded: ClientMessage = deserialize(&bytes).expect("deserialize failed");
    assert_eq!(format!("{:?}", msg), format!("{:?}", decoded));
}

#[test]
fn test_frame_message() {
    let payload = b"hello world";
    let framed = frame_message(payload);

    // Check length prefix
    assert_eq!(framed.len(), 4 + payload.len());
    let length = u32::from_be_bytes([framed[0], framed[1], framed[2], framed[3]]);
    assert_eq!(length, payload.len() as u32);

    // Check payload
    assert_eq!(&framed[4..], payload);
}

#[test]
fn test_unframe_message_complete() {
    let payload = b"hello world";
    let framed = frame_message(payload);

    let result = unframe_message(&framed).expect("unframe failed");
    assert!(result.is_some());

    let (decoded_payload, remaining) = result.unwrap();
    assert_eq!(decoded_payload, payload);
    assert_eq!(remaining.len(), 0);
}

#[test]
fn test_unframe_message_incomplete() {
    let payload = b"hello world";
    let framed = frame_message(payload);

    // Only provide length prefix
    let result = unframe_message(&framed[0..4]).expect("unframe failed");
    assert!(result.is_none());

    // Only provide partial message
    let result = unframe_message(&framed[0..8]).expect("unframe failed");
    assert!(result.is_none());
}

#[test]
fn test_unframe_message_insufficient_header() {
    // Less than 4 bytes
    let result = unframe_message(&[0, 1, 2]).expect("unframe failed");
    assert!(result.is_none());

    let result = unframe_message(&[]).expect("unframe failed");
    assert!(result.is_none());
}

#[test]
fn test_unframe_message_multiple() {
    let payload1 = b"first";
    let payload2 = b"second message";

    let framed1 = frame_message(payload1);
    let framed2 = frame_message(payload2);

    let mut buffer = Vec::new();
    buffer.extend_from_slice(&framed1);
    buffer.extend_from_slice(&framed2);

    // Unframe first message
    let result = unframe_message(&buffer).expect("unframe failed");
    assert!(result.is_some());
    let (decoded1, remaining) = result.unwrap();
    assert_eq!(decoded1, payload1);

    // Unframe second message
    let result = unframe_message(remaining).expect("unframe failed");
    assert!(result.is_some());
    let (decoded2, remaining) = result.unwrap();
    assert_eq!(decoded2, payload2);
    assert_eq!(remaining.len(), 0);
}

#[test]
fn test_unframe_message_too_large() {
    // Create a frame with length exceeding MAX_MESSAGE_SIZE
    let oversized_length = MAX_MESSAGE_SIZE + 1;
    let mut buffer = Vec::new();
    buffer.extend_from_slice(&oversized_length.to_be_bytes());
    buffer.extend_from_slice(&[0u8; 100]); // Some dummy data

    let result = unframe_message(&buffer);
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.to_string().contains("Message too large"));
}

#[test]
fn test_malformed_message_deserialization() {
    let invalid_data = b"this is not valid messagepack data";
    let result = deserialize::<ClientMessage>(invalid_data);
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.to_string().contains("Malformed message"));
}

#[test]
fn test_version_compatibility() {
    // Same version should be compatible
    let result = check_version_compatibility(PROTOCOL_VERSION, PROTOCOL_VERSION);
    assert!(result.is_ok());

    // Different versions should fail
    let result = check_version_compatibility(1, 2);
    assert!(result.is_err());

    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(err_msg.contains("Protocol version mismatch"));
    assert!(err_msg.contains("client=1"));
    assert!(err_msg.contains("server=2"));
}

#[test]
fn test_serialize_and_frame() {
    let msg = ClientMessage::Input {
        data: b"test".to_vec(),
    };

    let framed = serialize_and_frame(&msg).expect("serialize_and_frame failed");

    // Should have length prefix
    assert!(framed.len() >= 4);

    // Verify we can unframe it
    let result = unframe_message(&framed).expect("unframe failed");
    assert!(result.is_some());

    let (payload, _) = result.unwrap();
    let decoded: ClientMessage = deserialize(&payload).expect("deserialize failed");
    assert_eq!(format!("{:?}", msg), format!("{:?}", decoded));
}

#[test]
fn test_unframe_and_deserialize() {
    let msg = ClientMessage::Input {
        data: b"test".to_vec(),
    };

    let framed = serialize_and_frame(&msg).expect("serialize_and_frame failed");

    // Test complete message
    let result: Result<Option<(ClientMessage, usize)>, _> = unframe_and_deserialize(&framed);
    assert!(result.is_ok());

    let (decoded, consumed) = result.unwrap().unwrap();
    assert_eq!(format!("{:?}", msg), format!("{:?}", decoded));
    assert_eq!(consumed, framed.len());

    // Test incomplete message
    let result: Result<Option<(ClientMessage, usize)>, _> = unframe_and_deserialize(&framed[0..4]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_full_protocol_flow() {
    // Simulate a client-server handshake
    let client_hello = ClientMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
    };

    // Client serializes and frames
    let framed_hello = serialize_and_frame(&client_hello).expect("serialize failed");

    // Server receives and unframes
    let (decoded_hello, _): (ClientMessage, usize) = unframe_and_deserialize(&framed_hello)
        .expect("unframe failed")
        .unwrap();

    // Verify handshake
    if let ClientMessage::Hello {
        protocol_version: client_version,
    } = decoded_hello
    {
        check_version_compatibility(client_version, PROTOCOL_VERSION)
            .expect("version check failed");
    } else {
        panic!("Expected Hello message");
    }

    // Server responds with Welcome
    let server_welcome = ServerMessage::Welcome {
        session_id: uuid::Uuid::new_v4(),
        protocol_version: PROTOCOL_VERSION,
    };

    let framed_welcome = serialize_and_frame(&server_welcome).expect("serialize failed");

    // Client receives
    let (decoded_welcome, _): (ServerMessage, usize) = unframe_and_deserialize(&framed_welcome)
        .expect("unframe failed")
        .unwrap();

    assert!(matches!(decoded_welcome, ServerMessage::Welcome { .. }));
}
