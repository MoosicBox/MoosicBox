# Basic Playback Example

## Summary

This example demonstrates the fundamental usage of `moosicbox_player` for local audio playback. It shows how to set up a `LocalPlayer`, create a `PlaybackHandler`, play audio files, and control playback with standard operations like pause, resume, seek, and volume adjustment.

## What This Example Demonstrates

- Creating a `LocalPlayer` with audio output configuration
- Setting up a `PlaybackHandler` for playback control
- Playing a local FLAC audio file
- Pausing and resuming playback
- Seeking to specific positions in a track
- Adjusting playback volume dynamically
- Registering and handling playback events
- Properly stopping playback

## Prerequisites

Before running this example, you should:

- Have Rust and Cargo installed (1.70+)
- Have a FLAC audio file available for testing
- Understand basic async Rust (`async`/`await`)
- Have audio output hardware available on your system

## Running the Example

You need to provide a path to an audio file using the `AUDIO_FILE` environment variable:

```bash
# With a FLAC file
AUDIO_FILE=/path/to/your/audio.flac cargo run --manifest-path packages/player/examples/basic_playback/Cargo.toml

# With a different audio file (note: this example is configured for FLAC)
AUDIO_FILE=/path/to/your/audio.flac cargo run --manifest-path packages/player/examples/basic_playback/Cargo.toml
```

If you run without setting `AUDIO_FILE`, the example will exit gracefully with instructions.

## Expected Output

When you run the example with a valid audio file, you should see output like:

```
🎵 MoosicBox Player - Basic Playback Example
============================================

📡 Registering playback event listener...
🔊 Initializing audio output...
🎹 Creating LocalPlayer...
🎮 Creating PlaybackHandler...

📀 Creating sample track...
Using audio file: /path/to/your/audio.flac

🎵 Starting playback...
▶️  Playback started
✅ Playback started successfully!

⏸️  Pausing playback...
⏸️  Playback stopped
▶️  Resuming playback...
▶️  Playback started
⏩ Seeking to 10 seconds...
Seek position: 10.00s
🔊 Changing volume to 50%...
Volume: 0.50

⏹️  Stopping playback...
⏸️  Playback stopped

✨ Example completed successfully!

📝 This example demonstrated:
   ✓ Setting up a LocalPlayer with audio output
   ✓ Creating and using a PlaybackHandler
   ✓ Playing an audio track
   ✓ Pausing and resuming playback
   ✓ Seeking to a specific position
   ✓ Adjusting volume
   ✓ Handling playback events
   ✓ Stopping playback
```

You should also hear the audio file playing through your speakers/headphones, with appropriate pauses, seeks, and volume changes.

## Code Walkthrough

### 1. Initialization and Event Registration

```rust
// Initialize logging
env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

// Register event listener for playback state changes
on_playback_event(playback_event_handler);
```

We start by setting up logging and registering an event handler that will be called whenever playback state changes (play, pause, seek, volume, etc.).

### 2. Audio Output Setup

```rust
// Get the default audio output factory
let output_factory = default_output_factory()
    .await
    .ok_or("No default audio output available")?;
```

The `default_output_factory()` function detects and initializes the best available audio output backend for your platform (CPAL, JACK, ASIO, or Oboe).

### 3. LocalPlayer Creation

```rust
// Create a local player
let local_player = LocalPlayer::new(PlayerSource::Local, None)
    .await?
    .with_output(output_factory);
```

`LocalPlayer` is the primary implementation of the `Player` trait for local audio playback. We create it with:

- `PlayerSource::Local` - indicates we're playing local files
- `None` - uses default playback type (auto-detects file vs stream)
- `.with_output()` - attaches the audio output factory

### 4. PlaybackHandler Setup

```rust
let playback = local_player.playback.clone();
let output = local_player.output.clone();
let mut handler = PlaybackHandler::new(local_player)
    .with_playback(playback)
    .with_output(output);
```

`PlaybackHandler` is the high-level API for controlling playback. We clone the shared playback state and output factory from the player and attach them to the handler.

### 5. Creating a Track

```rust
let track = Track {
    id: 1.into(),
    title: "Sample Track".to_string(),
    duration: 180.0,
    file: Some(sample_file.clone()),
    format: Some(AudioFormat::Flac),
    ..Default::default()
};
```

A `Track` represents an audio file or stream. Key fields:

- `id` - unique track identifier
- `file` - path to the local audio file
- `format` - audio format (FLAC, MP3, AAC, Opus, or Source)
- `duration` - track length in seconds

### 6. Starting Playback

```rust
handler.play_track(
    session_id,
    profile.clone(),
    track.clone(),
    None,                               // No initial seek
    Some(volume),                       // Set volume to 80%
    PlaybackQuality {
        format: AudioFormat::Source,    // Use source format (FLAC)
    },
    None,                               // No specific playback target
    Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
).await?;
```

The `play_track()` method starts playback with the specified parameters:

- `session_id` - identifies the playback session
- `profile` - user profile for the session
- `track` - the track to play
- `seek` - optional starting position (None = start from beginning)
- `volume` - playback volume (0.0 to 1.0)
- `PlaybackQuality` - output format (Source keeps original format)
- `playback_target` - optional target device/zone
- `retry_options` - retry behavior for failed operations

### 7. Playback Controls

```rust
// Pause
handler.pause(Some(DEFAULT_PLAYBACK_RETRY_OPTIONS)).await?;

// Resume
handler.resume(Some(DEFAULT_PLAYBACK_RETRY_OPTIONS)).await?;

// Seek to 10 seconds
handler.seek(10.0, Some(DEFAULT_PLAYBACK_RETRY_OPTIONS)).await?;

// Change volume via update_playback
handler.update_playback(
    true,           // modify_playback
    None,           // play
    None,           // stop
    None,           // playing
    None,           // position
    None,           // seek
    Some(0.5),      // volume (50%)
    None,           // tracks
    None,           // quality
    Some(session_id),
    Some(profile),
    None,           // playback_target
    true,           // trigger_event
    Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
).await?;

// Stop
handler.stop(Some(DEFAULT_PLAYBACK_RETRY_OPTIONS)).await?;
```

The handler provides methods for all standard playback operations. The `update_playback()` method is a powerful, flexible way to modify multiple aspects of playback state at once.

### 8. Event Handling

```rust
fn playback_event_handler(update: &UpdateSession, playback: &Playback) {
    if let Some(playing) = update.playing {
        log::info!("Playback state changed: {}", playing);
    }
    if let Some(seek) = update.seek {
        log::info!("Seek position: {:.2}s", seek);
    }
    // ... handle other fields
}
```

The event handler receives `UpdateSession` (changes) and `Playback` (current state). This allows you to react to playback state changes in real-time.

## Key Concepts

### PlaybackHandler vs Player

- **`Player`** - Low-level trait for implementing playback backends
- **`PlaybackHandler`** - High-level API that manages a `Player` instance
- Use `PlaybackHandler` for application code; implement `Player` only when creating custom backends

### Session-Based Playback

The player uses sessions to manage concurrent playback:

- Each session has a unique `session_id`
- Sessions track their own state (playing, position, volume, etc.)
- Multiple sessions can exist, but typically only one plays at a time
- Sessions enable features like playlist management and playback resumption

### Playback Quality and Format Conversion

`PlaybackQuality` controls the output audio format:

```rust
PlaybackQuality { format: AudioFormat::Source }  // No conversion
PlaybackQuality { format: AudioFormat::Flac }    // Convert to FLAC
PlaybackQuality { format: AudioFormat::Opus }    // Convert to Opus
```

When the output format differs from the source, the player uses the signal chain to transcode in real-time.

### Retry Options

`PlaybackRetryOptions` configures automatic retry behavior:

```rust
PlaybackRetryOptions {
    max_attempts: 10,
    retry_delay: Duration::from_millis(500),
}
```

This is useful for handling transient errors in network playback or audio output issues.

### Atomic Volume Control

Volume is stored in an `Arc<AtomicF64>` for thread-safe, lock-free access. This allows smooth volume changes even during active playback without audio glitches.

## Testing the Example

To test this example:

1. **Find a FLAC audio file** on your system
2. **Run with the file path**:
    ```bash
    AUDIO_FILE=/path/to/audio.flac cargo run --manifest-path packages/player/examples/basic_playback/Cargo.toml
    ```
3. **Listen for**:
    - Audio starts playing
    - Brief pause after 2 seconds
    - Resume after 1 second
    - Seek jump to 10 seconds
    - Volume drop to 50%
    - Final stop
4. **Check the logs** for event notifications matching each operation

## Troubleshooting

### "No default audio output available"

**Problem**: Your system doesn't have a supported audio output backend.

**Solution**:

- Ensure you have audio hardware connected
- On Linux, you may need ALSA or PulseAudio libraries
- On macOS/Windows, built-in audio should work
- Try installing `libasound2-dev` (Linux) or equivalent

### "Audio file not found"

**Problem**: The `AUDIO_FILE` path is incorrect or the file doesn't exist.

**Solution**:

- Verify the file path is correct and absolute
- Ensure the file is readable by your user
- Use a FLAC file (this example is configured for FLAC)

### No audio output but no errors

**Problem**: Playback appears to work but no sound is heard.

**Solution**:

- Check your system volume is not muted
- Verify the correct audio output device is selected
- Try increasing the volume in the example (change `0.8` to `1.0`)

### Compilation errors about missing features

**Problem**: Required feature flags are not enabled.

**Solution**:

- Ensure the example's `Cargo.toml` has `decoder-flac` and `local` features
- These are configured correctly in this example

## Related Examples

This is currently the only example for `moosicbox_player`. Future examples may include:

- Remote streaming playback
- Playlist and queue management
- Multiple concurrent sessions
- Custom `Player` trait implementation
- Integration with the HTTP API
