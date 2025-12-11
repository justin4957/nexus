use nexus::channel::{ChannelConfig, PtyChannel};
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn pty_echoes_output() -> anyhow::Result<()> {
    let mut channel = PtyChannel::spawn(ChannelConfig::new("test-pty")).await?;
    let mut output = channel
        .take_output_receiver()
        .expect("output receiver should be available");

    channel.write(b"echo hello\n").await?;

    let mut buffer = Vec::new();
    let mut found = false;

    for _ in 0..10 {
        if let Ok(Some(chunk)) = timeout(Duration::from_secs(2), output.recv()).await {
            buffer.extend_from_slice(&chunk);
            if buffer.windows(b"hello".len()).any(|w| w == b"hello") {
                found = true;
                break;
            }
        }
    }

    channel.kill().await.ok();
    assert!(
        found,
        "PTY output did not contain 'hello'; got: {:?}",
        String::from_utf8_lossy(&buffer)
    );

    Ok(())
}
