//! Integration tests for protocol serialization

use nexus::protocol::{deserialize, serialize, ClientMessage, ServerMessage};

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
