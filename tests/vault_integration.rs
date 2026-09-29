use cosmic_notes::core::{Vault, VaultEvent, VaultWatcher};
use std::fs;
use std::path::Path;
use std::time::Duration;
use tempfile::tempdir;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_watcher_write_echo_cancellation() {
    let dir = tempdir().unwrap();
    let vault = Vault::open(dir.path()).unwrap();

    let (tx, mut rx) = mpsc::unbounded_channel::<VaultEvent>();
    let _watcher = VaultWatcher::start(
        dir.path().to_path_buf(),
        vault.write_echo_cache().clone(),
        tx,
        Duration::from_millis(50),
    )
    .expect("Failed to start watcher");

    // 1. Vault write (app-initiated save): registers write-echo token
    let note_path = Path::new("echo_test.md");
    vault
        .write_note(note_path, "# Internal Write\nThis is saved by app.")
        .unwrap();

    // Sleep to let debounced watcher process
    tokio::time::sleep(Duration::from_millis(250)).await;

    // The app-initiated save should be CANCELLED by the write-echo cache, so rx should be empty!
    assert!(
        rx.try_recv().is_err(),
        "Expected write-echo event to be dropped, but an event was received!"
    );

    // 2. External write (simulating external editor, Git pull, or Syncthing):
    // Directly write using std::fs without registering in write_echo_cache
    let external_path = dir.path().join("external.md");
    fs::write(&external_path, "# External Note\nModified by another program.").unwrap();

    // Sleep to let debouncer trigger
    tokio::time::sleep(Duration::from_millis(250)).await;

    // The external write MUST be detected and forwarded as a VaultEvent
    let event = rx.try_recv().expect("Expected to receive external file event");
    match event {
        VaultEvent::Created(rel) | VaultEvent::Modified(rel) => {
            assert_eq!(rel, Path::new("external.md"));
        }
        _ => panic!("Unexpected event kind: {:?}", event),
    }
}
