use std::path::PathBuf;
use tokio::net::UnixListener;
#[tokio::main]
async fn main() {
    let dir = std::env::temp_dir();
    let path: PathBuf = dir.join("socket_test.sock");
    if path.exists() { let _ = std::fs::remove_file(&path); }
    match UnixListener::bind(&path) {
        Ok(_) => println!("bound: {:?}", path),
        Err(e) => println!("bind failed: {:?} -> {}", path, e),
    }
}
