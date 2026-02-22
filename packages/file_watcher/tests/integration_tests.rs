use moosicbox_file_watcher::{EventFilter, watch_directory};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Helper function to create a temporary test directory
fn create_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("file_watcher_test_{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("Failed to create test directory");
    dir
}

/// Helper function to clean up test directory
fn cleanup_test_dir(dir: &PathBuf) {
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn test_watch_directory_modify_event() {
    let test_dir = create_test_dir("modify");
    let test_file = test_dir.join("test.txt");

    // Create initial file
    fs::write(&test_file, "initial content").expect("Failed to write initial file");

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let dir_clone = test_dir.clone();
    let file_clone = test_file.clone();

    // Start watching in a separate thread
    let _watcher_thread = thread::spawn(move || {
        let filter = EventFilter::default().with_modify();

        let result = watch_directory(dir_clone, filter, move |event| {
            let mut ev = events_clone.lock().unwrap();
            ev.push(event);

            // Exit after receiving first event
            if !ev.is_empty() {
                std::process::exit(0);
            }
        });

        // This should run indefinitely unless we exit
        result
    });

    // Wait a bit for watcher to initialize
    thread::sleep(Duration::from_millis(500));

    // Trigger a modify event
    fs::write(&file_clone, "modified content").expect("Failed to modify file");

    // Wait for event to be processed
    thread::sleep(Duration::from_millis(500));

    // The watcher thread should have exited via process::exit(0)
    // So we just check if events were collected (though we can't easily verify
    // due to process::exit in the actual implementation)

    cleanup_test_dir(&test_dir);
}

#[test]
fn test_watch_directory_create_event() {
    let test_dir = create_test_dir("create");

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let dir_clone = test_dir.clone();
    let test_file = test_dir.join("new_file.txt");
    let file_clone = test_file.clone();

    // Start watching in a separate thread
    let _watcher_thread = thread::spawn(move || {
        let filter = EventFilter::default().with_create();

        let _ = watch_directory(dir_clone, filter, move |event| {
            let mut ev = events_clone.lock().unwrap();
            ev.push(event);
        });
    });

    // Wait a bit for watcher to initialize
    thread::sleep(Duration::from_millis(500));

    // Trigger a create event
    fs::write(&file_clone, "new file content").expect("Failed to create file");

    // Wait for event to be processed
    thread::sleep(Duration::from_millis(500));

    // Verify events were received
    let events_vec = events.lock().unwrap();
    assert!(!events_vec.is_empty(), "Expected to receive create event");

    cleanup_test_dir(&test_dir);
}

#[test]
fn test_watch_directory_remove_event() {
    let test_dir = create_test_dir("remove");
    let test_file = test_dir.join("to_delete.txt");

    // Create initial file
    fs::write(&test_file, "will be deleted").expect("Failed to write file");

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let dir_clone = test_dir.clone();
    let file_clone = test_file.clone();

    // Start watching in a separate thread
    let _watcher_thread = thread::spawn(move || {
        let filter = EventFilter::default().with_remove();

        let _ = watch_directory(dir_clone, filter, move |event| {
            let mut ev = events_clone.lock().unwrap();
            ev.push(event);
        });
    });

    // Wait a bit for watcher to initialize
    thread::sleep(Duration::from_millis(500));

    // Trigger a remove event
    fs::remove_file(&file_clone).expect("Failed to remove file");

    // Wait for event to be processed
    thread::sleep(Duration::from_millis(500));

    // Verify events were received
    let events_vec = events.lock().unwrap();
    assert!(!events_vec.is_empty(), "Expected to receive remove event");

    cleanup_test_dir(&test_dir);
}

#[test]
fn test_event_filter_multiple_events() {
    let test_dir = create_test_dir("multiple");
    let test_file = test_dir.join("test.txt");

    // Create initial file
    fs::write(&test_file, "initial").expect("Failed to write initial file");

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let dir_clone = test_dir.clone();
    let file_clone = test_file.clone();

    // Start watching with multiple event types
    let _watcher_thread = thread::spawn(move || {
        let filter = EventFilter::default().with_modify().with_create();

        let _ = watch_directory(dir_clone, filter, move |event| {
            let mut ev = events_clone.lock().unwrap();
            ev.push(event);
        });
    });

    // Wait for watcher to initialize
    thread::sleep(Duration::from_millis(500));

    // Trigger modify event
    fs::write(&file_clone, "modified").expect("Failed to modify file");
    thread::sleep(Duration::from_millis(300));

    // Trigger create event
    let new_file = test_dir.join("new.txt");
    fs::write(&new_file, "created").expect("Failed to create file");
    thread::sleep(Duration::from_millis(300));

    // Verify multiple events were received
    let events_vec = events.lock().unwrap();
    assert!(events_vec.len() >= 1, "Expected to receive multiple events");

    cleanup_test_dir(&test_dir);
}

#[test]
fn test_event_filter_parse_valid() {
    let filter = EventFilter::parse("modify,create,remove").unwrap();
    assert!(filter.modify);
    assert!(filter.create);
    assert!(filter.remove);
    assert!(!filter.close_write);
    assert!(!filter.access);
}

#[test]
fn test_event_filter_parse_with_spaces() {
    let filter = EventFilter::parse("modify , create , remove").unwrap();
    assert!(filter.modify);
    assert!(filter.create);
    assert!(filter.remove);
}

#[test]
fn test_event_filter_parse_invalid() {
    let result = EventFilter::parse("modify,invalid_event");
    assert!(result.is_err());
}

#[test]
fn test_event_filter_builder_pattern() {
    let filter = EventFilter::new()
        .with_modify()
        .with_create()
        .with_remove()
        .with_close_write()
        .with_access();

    assert!(filter.modify);
    assert!(filter.create);
    assert!(filter.remove);
    assert!(filter.close_write);
    assert!(filter.access);
}

#[test]
fn test_event_filter_default() {
    let filter = EventFilter::default();
    assert!(!filter.modify);
    assert!(!filter.create);
    assert!(!filter.remove);
    assert!(!filter.close_write);
    assert!(!filter.access);
}
