# moosicbox_file_watcher

Cross-platform file watcher for monitoring filesystem changes.

## Features

- **Cross-platform**: Works on Linux, macOS, and Windows
- **Fast**: Efficient filesystem monitoring using the `notify` crate
- **Flexible**: Support for multiple event types (modify, create, remove, etc.)
- **CLI**: Optional command-line interface for use in scripts and workflows

## Usage

### As a Library

```rust
use moosicbox_file_watcher::{watch_directory, EventFilter};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new("/path/to/watch");
    let filter = EventFilter::default()
        .with_modify()
        .with_create();

    watch_directory(path, filter, |event| {
        println!("Event: {:?}", event);
    })?;

    Ok(())
}
```

### As a CLI Tool

```bash
# Monitor a directory for modifications
moosicbox-file-watcher -m -e modify,close_write /path/to/watch

# Quiet mode (no output)
moosicbox-file-watcher -q -m -e modify /path/to/watch

# Background execution
moosicbox-file-watcher -q -m -e modify /path/to/watch &
```

## License

MPL-2.0
