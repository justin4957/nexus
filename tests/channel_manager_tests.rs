//! Integration tests for ChannelManager

use nexus::channel::{ChannelConfig, ChannelManager, ChannelManagerEvent, ChannelState};
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn test_create_channel() -> anyhow::Result<()> {
    let (event_tx, _event_rx) = mpsc::channel(32);
    let mut manager = ChannelManager::new(event_tx);

    manager.create_channel(ChannelConfig::new("test1")).await?;

    let channels = manager.list_channels();
    assert_eq!(channels.len(), 1);
    assert!(channels.contains(&"test1".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_create_multiple_channels() -> anyhow::Result<()> {
    let (event_tx, _event_rx) = mpsc::channel(32);
    let mut manager = ChannelManager::new(event_tx);

    manager.create_channel(ChannelConfig::new("build")).await?;
    manager.create_channel(ChannelConfig::new("test")).await?;
    manager.create_channel(ChannelConfig::new("server")).await?;

    let mut channels = manager.list_channels();
    channels.sort();
    assert_eq!(channels, vec!["build", "server", "test"]);

    Ok(())
}

#[tokio::test]
async fn test_duplicate_channel_name_prevention() -> anyhow::Result<()> {
    let (event_tx, _event_rx) = mpsc::channel(32);
    let mut manager = ChannelManager::new(event_tx);

    manager
        .create_channel(ChannelConfig::new("duplicate"))
        .await?;

    let result = manager
        .create_channel(ChannelConfig::new("duplicate"))
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already exists"));

    Ok(())
}

#[tokio::test]
async fn test_acceptance_criteria() -> anyhow::Result<()> {
    let (event_tx, _event_rx) = mpsc::channel(32);
    let mut manager = ChannelManager::new(event_tx);

    manager.create_channel(ChannelConfig::new("build")).await?;
    manager.create_channel(ChannelConfig::new("test")).await?;

    let mut channels = manager.list_channels();
    channels.sort();
    assert_eq!(channels, vec!["build", "test"]);

    manager.send_input_to("build", b"npm run build\n").await?;

    Ok(())
}

#[tokio::test]
async fn test_first_channel_is_active() -> anyhow::Result<()> {
    let (event_tx, _event_rx) = mpsc::channel(32);
    let mut manager = ChannelManager::new(event_tx);

    manager.create_channel(ChannelConfig::new("first")).await?;
    assert_eq!(manager.active_channel(), Some("first"));

    manager.create_channel(ChannelConfig::new("second")).await?;
    assert_eq!(manager.active_channel(), Some("first"));

    Ok(())
}

#[tokio::test]
async fn test_switch_active_channel() -> anyhow::Result<()> {
    let (event_tx, _event_rx) = mpsc::channel(32);
    let mut manager = ChannelManager::new(event_tx);

    manager.create_channel(ChannelConfig::new("chan1")).await?;
    manager.create_channel(ChannelConfig::new("chan2")).await?;

    assert_eq!(manager.active_channel(), Some("chan1"));

    manager.switch_active("chan2")?;
    assert_eq!(manager.active_channel(), Some("chan2"));

    Ok(())
}

#[tokio::test]
async fn test_switch_to_nonexistent_channel() -> anyhow::Result<()> {
    let (event_tx, _event_rx) = mpsc::channel(32);
    let mut manager = ChannelManager::new(event_tx);

    manager.create_channel(ChannelConfig::new("exists")).await?;

    let result = manager.switch_active("nonexistent");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));

    Ok(())
}

#[tokio::test]
async fn test_send_input_to_active_channel() -> anyhow::Result<()> {
    let (event_tx, mut event_rx) = mpsc::channel(32);
    let mut manager = ChannelManager::new(event_tx);

    manager.create_channel(ChannelConfig::new("active")).await?;

    manager.send_input(b"echo test\n").await?;

    // Verify output event is received
    let mut found_output = false;
    for _ in 0..10 {
        if let Ok(Some(ChannelManagerEvent::Output { channel_name, .. })) =
            timeout(Duration::from_secs(2), event_rx.recv()).await
        {
            if channel_name == "active" {
                found_output = true;
                break;
            }
        }
    }

    assert!(found_output, "Should receive output from active channel");

    Ok(())
}

#[tokio::test]
async fn test_send_input_to_specific_channel() -> anyhow::Result<()> {
    let (event_tx, mut event_rx) = mpsc::channel(32);
    let mut manager = ChannelManager::new(event_tx);

    manager.create_channel(ChannelConfig::new("chan1")).await?;
    manager.create_channel(ChannelConfig::new("chan2")).await?;

    assert_eq!(manager.active_channel(), Some("chan1"));

    manager.send_input_to("chan2", b"echo test\n").await?;

    // Verify output event is received from chan2
    let mut found_output = false;
    for _ in 0..10 {
        if let Ok(Some(ChannelManagerEvent::Output { channel_name, .. })) =
            timeout(Duration::from_secs(2), event_rx.recv()).await
        {
            if channel_name == "chan2" {
                found_output = true;
                break;
            }
        }
    }

    assert!(found_output, "Should receive output from chan2");

    Ok(())
}

#[tokio::test]
async fn test_send_input_to_nonexistent_channel() -> anyhow::Result<()> {
    let (event_tx, _event_rx) = mpsc::channel(32);
    let mut manager = ChannelManager::new(event_tx);

    manager.create_channel(ChannelConfig::new("exists")).await?;

    let result = manager.send_input_to("nonexistent", b"test\n").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));

    Ok(())
}

#[tokio::test]
async fn test_kill_channel() -> anyhow::Result<()> {
    let (event_tx, mut event_rx) = mpsc::channel(32);
    let mut manager = ChannelManager::new(event_tx);

    manager.create_channel(ChannelConfig::new("tokill")).await?;

    manager.kill_channel("tokill").await?;

    // Wait for state change event
    let mut killed = false;
    for _ in 0..10 {
        if let Ok(Some(ChannelManagerEvent::StateChanged {
            channel_name,
            state,
        })) = timeout(Duration::from_secs(2), event_rx.recv()).await
        {
            if channel_name == "tokill" && matches!(state, ChannelState::Killed) {
                killed = true;
                break;
            }
        }
    }

    assert!(killed, "Channel should be killed");

    Ok(())
}

#[tokio::test]
async fn test_kill_channel_switches_active() -> anyhow::Result<()> {
    let (event_tx, _event_rx) = mpsc::channel(32);
    let mut manager = ChannelManager::new(event_tx);

    manager.create_channel(ChannelConfig::new("first")).await?;
    manager.create_channel(ChannelConfig::new("second")).await?;

    assert_eq!(manager.active_channel(), Some("first"));

    manager.kill_channel("first").await?;

    assert_eq!(
        manager.active_channel(),
        Some("second"),
        "Active channel should switch to second after killing first"
    );

    Ok(())
}

#[tokio::test]
async fn test_subscribe_to_channels() -> anyhow::Result<()> {
    let (event_tx, _event_rx) = mpsc::channel(32);
    let mut manager = ChannelManager::new(event_tx);

    manager.create_channel(ChannelConfig::new("chan1")).await?;
    manager.create_channel(ChannelConfig::new("chan2")).await?;

    assert!(
        manager.is_subscribed("chan1"),
        "First channel auto-subscribed"
    );
    assert!(!manager.is_subscribed("chan2"));

    manager.subscribe(&["chan2".to_string()]);

    assert!(manager.is_subscribed("chan2"));

    Ok(())
}

#[tokio::test]
async fn test_unsubscribe_from_channels() -> anyhow::Result<()> {
    let (event_tx, _event_rx) = mpsc::channel(32);
    let mut manager = ChannelManager::new(event_tx);

    manager.create_channel(ChannelConfig::new("chan1")).await?;
    manager.create_channel(ChannelConfig::new("chan2")).await?;

    manager.subscribe(&["chan2".to_string()]);

    assert!(manager.is_subscribed("chan1"));
    assert!(manager.is_subscribed("chan2"));

    manager.unsubscribe(&["chan1".to_string()]);

    assert!(!manager.is_subscribed("chan1"));
    assert!(manager.is_subscribed("chan2"));

    Ok(())
}

#[tokio::test]
async fn test_event_emission_on_create() -> anyhow::Result<()> {
    let (event_tx, mut event_rx) = mpsc::channel(32);
    let mut manager = ChannelManager::new(event_tx);

    manager
        .create_channel(ChannelConfig::new("newevent"))
        .await?;

    // Should receive a StateChanged event with Running state
    let mut found_running = false;
    for _ in 0..10 {
        if let Ok(Some(ChannelManagerEvent::StateChanged {
            channel_name,
            state,
        })) = timeout(Duration::from_millis(500), event_rx.recv()).await
        {
            if channel_name == "newevent" && matches!(state, ChannelState::Running) {
                found_running = true;
                break;
            }
        }
    }

    assert!(found_running, "Should emit Running state event on create");

    Ok(())
}

#[tokio::test]
async fn test_channel_exit_state_change() -> anyhow::Result<()> {
    let (event_tx, mut event_rx) = mpsc::channel(32);
    let mut manager = ChannelManager::new(event_tx);

    // Create a channel with default shell and send exit command
    manager.create_channel(ChannelConfig::new("exiter")).await?;

    // Send exit command to make it exit
    manager.send_input_to("exiter", b"exit\n").await?;

    // Wait for output and exit events
    let mut found_exit = false;
    for _ in 0..20 {
        if let Ok(Some(ChannelManagerEvent::StateChanged {
            channel_name,
            state,
        })) = timeout(Duration::from_secs(2), event_rx.recv()).await
        {
            if channel_name == "exiter" && matches!(state, ChannelState::Exited(_)) {
                found_exit = true;
                break;
            }
        }
    }

    assert!(found_exit, "Should emit Exited state event");

    Ok(())
}

#[tokio::test]
async fn test_resize_all_channels() -> anyhow::Result<()> {
    let (event_tx, _event_rx) = mpsc::channel(32);
    let mut manager = ChannelManager::new(event_tx);

    manager.create_channel(ChannelConfig::new("r1")).await?;
    manager.create_channel(ChannelConfig::new("r2")).await?;

    manager.resize_all(120, 40).await?;

    Ok(())
}
