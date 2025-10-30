#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic audio playback example for `moosicbox_player`.
//!
//! This example demonstrates how to:
//! - Set up a local player with audio output
//! - Create a `PlaybackHandler` for controlling playback
//! - Play a FLAC audio file
//! - Control playback (pause, resume, seek, stop)
//! - Handle playback events

use moosicbox_audio_output::default_output_factory;
use moosicbox_music_models::{AudioFormat, PlaybackQuality, Track};
use moosicbox_player::{
    DEFAULT_PLAYBACK_RETRY_OPTIONS, Playback, PlaybackHandler, PlayerSource, local::LocalPlayer,
    on_playback_event,
};
use moosicbox_session::models::UpdateSession;

/// Event handler that prints playback state changes
fn playback_event_handler(update: &UpdateSession, playback: &Playback) {
    if let Some(playing) = update.playing {
        if playing {
            log::info!("▶️  Playback started");
        } else {
            log::info!("⏸️  Playback stopped");
        }
    }
    if let Some(position) = update.position {
        log::info!("Track position: {position}");
    }
    if let Some(seek) = update.seek {
        log::info!("Seek position: {seek:.2}s");
    }
    if let Some(volume) = update.volume {
        log::info!("Volume: {volume:.2}");
    }

    log::debug!("Playback state: id={}", playback.id);
}

#[switchy_async::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("🎵 MoosicBox Player - Basic Playback Example");
    log::info!("============================================\n");

    // Step 1: Register event listener for playback state changes
    log::info!("📡 Registering playback event listener...");
    on_playback_event(playback_event_handler);

    // Step 2: Get the default audio output factory
    log::info!("🔊 Initializing audio output...");
    let output_factory = default_output_factory()
        .await
        .ok_or("No default audio output available")?;

    // Step 3: Create a local player
    log::info!("🎹 Creating LocalPlayer...");
    let local_player = LocalPlayer::new(PlayerSource::Local, None)
        .await?
        .with_output(output_factory);

    // Step 4: Create a playback handler
    log::info!("🎮 Creating PlaybackHandler...");
    let playback = local_player.playback.clone();
    let output = local_player.output.clone();
    let mut handler = PlaybackHandler::new(local_player)
        .with_playback(playback)
        .with_output(output);

    // Step 5: Create a sample track
    // Note: In a real application, you would load this from a database or API
    log::info!("\n📀 Creating sample track...");

    // This example expects a FLAC file at this path
    let sample_file =
        std::env::var("AUDIO_FILE").unwrap_or_else(|_| "/path/to/sample.flac".to_string());

    log::info!("Using audio file: {sample_file}");

    let track = Track {
        id: 1.into(),
        title: "Sample Track".to_string(),
        duration: 180.0,
        file: Some(sample_file.clone()),
        format: Some(AudioFormat::Flac),
        ..Default::default()
    };

    // Check if the file exists
    if !std::path::Path::new(&sample_file).exists() {
        log::warn!("\n⚠️  Audio file not found: {sample_file}");
        log::info!("\n💡 To run this example with an actual audio file:");
        log::info!(
            "   AUDIO_FILE=/path/to/your/audio.flac cargo run --manifest-path packages/player/examples/basic_playback/Cargo.toml"
        );
        log::info!("\nExiting without playing (file not found).");
        return Ok(());
    }

    // Step 6: Play the track
    log::info!("\n🎵 Starting playback...");
    let session_id = 1;
    let profile = "default".to_string();
    let volume = 0.8;

    handler
        .play_track(
            session_id,
            profile.clone(),
            track.clone(),
            None,         // No initial seek
            Some(volume), // Set volume to 80%
            PlaybackQuality {
                format: AudioFormat::Source, // Use source format (FLAC)
            },
            None,                                 // No specific playback target
            Some(DEFAULT_PLAYBACK_RETRY_OPTIONS), // Use default retry options
        )
        .await?;

    log::info!("✅ Playback started successfully!");

    // Wait for playback to start
    switchy_async::time::sleep(std::time::Duration::from_secs(2)).await;

    // Step 7: Demonstrate playback controls
    log::info!("\n⏸️  Pausing playback...");
    handler.pause(Some(DEFAULT_PLAYBACK_RETRY_OPTIONS)).await?;
    switchy_async::time::sleep(std::time::Duration::from_secs(1)).await;

    log::info!("▶️  Resuming playback...");
    handler.resume(Some(DEFAULT_PLAYBACK_RETRY_OPTIONS)).await?;
    switchy_async::time::sleep(std::time::Duration::from_secs(2)).await;

    log::info!("⏩ Seeking to 10 seconds...");
    handler
        .seek(10.0, Some(DEFAULT_PLAYBACK_RETRY_OPTIONS))
        .await?;
    switchy_async::time::sleep(std::time::Duration::from_secs(2)).await;

    log::info!("🔊 Changing volume to 50%...");
    handler
        .update_playback(
            true,      // modify_playback
            None,      // play
            None,      // stop
            None,      // playing
            None,      // position
            None,      // seek
            Some(0.5), // volume
            None,      // tracks
            None,      // quality
            Some(session_id),
            Some(profile),
            None, // playback_target
            true, // trigger_event
            Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
        )
        .await?;
    switchy_async::time::sleep(std::time::Duration::from_secs(2)).await;

    // Step 8: Stop playback
    log::info!("\n⏹️  Stopping playback...");
    handler.stop(Some(DEFAULT_PLAYBACK_RETRY_OPTIONS)).await?;

    log::info!("\n✨ Example completed successfully!");
    log::info!("\n📝 This example demonstrated:");
    log::info!("   ✓ Setting up a LocalPlayer with audio output");
    log::info!("   ✓ Creating and using a PlaybackHandler");
    log::info!("   ✓ Playing an audio track");
    log::info!("   ✓ Pausing and resuming playback");
    log::info!("   ✓ Seeking to a specific position");
    log::info!("   ✓ Adjusting volume");
    log::info!("   ✓ Handling playback events");
    log::info!("   ✓ Stopping playback");

    Ok(())
}
