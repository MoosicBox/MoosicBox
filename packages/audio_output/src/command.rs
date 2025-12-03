//! Command-based control interface for audio outputs.
//!
//! This module provides an async command system for controlling audio playback through
//! message passing. Audio outputs can be controlled from different threads or async contexts
//! using [`AudioHandle`], which sends [`AudioCommand`]s and receives [`AudioResponse`]s.

#![allow(clippy::module_name_repetitions)]

use std::fmt;
use thiserror::Error;

/// Commands that can be sent to control audio output.
#[derive(Debug, Clone)]
pub enum AudioCommand {
    /// Set the volume level (0.0 to 1.0)
    SetVolume(f64),
    /// Pause audio playback
    Pause,
    /// Resume audio playback
    Resume,
    /// Seek to the specified position in seconds
    Seek(f64),
    /// Flush buffered audio data
    Flush,
    /// Reset the audio output to its initial state
    Reset,
}

/// Response returned after executing an audio command.
#[derive(Debug, Clone)]
pub enum AudioResponse {
    /// Command executed successfully
    Success,
    /// Command execution failed with error message
    Error(String),
}

/// Message structure for sending commands through channels.
#[derive(Debug)]
pub struct CommandMessage {
    /// The command to execute
    pub command: AudioCommand,
    /// Optional channel for sending back the response
    pub response_sender: Option<flume::Sender<AudioResponse>>,
}

/// Errors that can occur during audio command operations.
#[derive(Debug, Error)]
pub enum AudioError {
    /// Command execution failed with error message
    #[error("Command error: {0}")]
    Command(String),
    /// Failed to send command through channel
    #[error("Channel send error")]
    ChannelSend,
    /// Failed to receive response through channel
    #[error("Channel receive error")]
    ChannelReceive,
    /// Received unexpected response type
    #[error("Unexpected response type")]
    UnexpectedResponse,
    /// Audio handle is not available
    #[error("Handle not available")]
    HandleNotAvailable,
}

impl From<flume::SendError<CommandMessage>> for AudioError {
    fn from(_: flume::SendError<CommandMessage>) -> Self {
        Self::ChannelSend
    }
}

impl From<flume::TrySendError<CommandMessage>> for AudioError {
    fn from(_: flume::TrySendError<CommandMessage>) -> Self {
        Self::ChannelSend
    }
}

impl From<flume::RecvError> for AudioError {
    fn from(_: flume::RecvError) -> Self {
        Self::ChannelReceive
    }
}

/// Handle for sending commands to an audio output from different threads or async contexts.
///
/// The handle uses channels to communicate with the audio output's command processor,
/// allowing safe control of playback from anywhere in the application.
pub struct AudioHandle {
    command_sender: flume::Sender<CommandMessage>,
}

impl fmt::Debug for AudioHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioHandle")
            .field("command_sender", &"<flume::Sender>")
            .finish()
    }
}

impl Clone for AudioHandle {
    fn clone(&self) -> Self {
        Self {
            command_sender: self.command_sender.clone(),
        }
    }
}

impl AudioHandle {
    /// Creates a new `AudioHandle` with the specified command sender.
    ///
    /// # Arguments
    /// * `command_sender` - Channel for sending commands to the audio output
    #[must_use]
    pub const fn new(command_sender: flume::Sender<CommandMessage>) -> Self {
        Self { command_sender }
    }

    /// Set the volume of the audio output
    ///
    /// # Errors
    ///
    /// * If the command processor fails to send the command
    /// * If the `AudioCommand::SetVolume` command fails to be processed by the command processor
    pub async fn set_volume(&self, volume: f64) -> Result<(), AudioError> {
        self.send_command_with_response(AudioCommand::SetVolume(volume))
            .await?;
        Ok(())
    }

    /// Pause the audio output
    ///
    /// # Errors
    ///
    /// * If the command processor fails to send the command
    /// * If the `AudioCommand::Pause` command fails to be processed by the command processor
    pub async fn pause(&self) -> Result<(), AudioError> {
        self.send_command_with_response(AudioCommand::Pause).await?;
        Ok(())
    }

    /// Resume the audio output
    ///
    /// # Errors
    ///
    /// * If the command processor fails to send the command
    /// * If the `AudioCommand::Resume` command fails to be processed by the command processor
    pub async fn resume(&self) -> Result<(), AudioError> {
        self.send_command_with_response(AudioCommand::Resume)
            .await?;
        Ok(())
    }

    /// Seek to the specified position in the audio output
    ///
    /// # Errors
    ///
    /// * If the command processor fails to send the command
    /// * If the `AudioCommand::Seek` command fails to be processed by the command processor
    pub async fn seek(&self, position: f64) -> Result<(), AudioError> {
        self.send_command_with_response(AudioCommand::Seek(position))
            .await?;
        Ok(())
    }

    /// Flush the audio output
    ///
    /// # Errors
    ///
    /// * If the command processor fails to send the command
    /// * If the `AudioCommand::Flush` command fails to be processed by the command processor
    pub async fn flush(&self) -> Result<(), AudioError> {
        self.send_command_with_response(AudioCommand::Flush).await?;
        Ok(())
    }

    /// Reset the audio output
    ///
    /// # Errors
    ///
    /// * If the command processor fails to send the command
    /// * If the `AudioCommand::Reset` command fails to be processed by the command processor
    pub async fn reset(&self) -> Result<(), AudioError> {
        self.send_command_with_response(AudioCommand::Reset).await?;
        Ok(())
    }

    /// Immediately set the volume of the audio output
    ///
    /// This is a fire-and-forget command that does not wait for a response.
    /// It is intended to be used in situations where the caller wants to set the volume of the audio output
    /// immediately, without waiting for the response from the command processor.
    ///
    /// # Errors
    ///
    /// * If the command processor fails to send the command
    pub fn set_volume_immediate(&self, volume: f64) -> Result<(), AudioError> {
        self.send_command_fire_and_forget(AudioCommand::SetVolume(volume))
    }

    /// Immediately pause the audio output
    ///
    /// This is a fire-and-forget command that does not wait for a response.
    /// It is intended to be used in situations where the caller wants to pause the audio output
    /// immediately, without waiting for the response from the command processor.
    ///
    /// # Errors
    ///
    /// * If the command processor fails to send the command
    pub fn pause_immediate(&self) -> Result<(), AudioError> {
        self.send_command_fire_and_forget(AudioCommand::Pause)
    }

    /// Immediately resume the audio output
    ///
    /// This is a fire-and-forget command that does not wait for a response.
    /// It is intended to be used in situations where the caller wants to resume the audio output
    /// immediately, without waiting for the response from the command processor.
    ///
    /// # Errors
    ///
    /// * If the command processor fails to send the command
    pub fn resume_immediate(&self) -> Result<(), AudioError> {
        self.send_command_fire_and_forget(AudioCommand::Resume)
    }

    async fn send_command_with_response(
        &self,
        command: AudioCommand,
    ) -> Result<AudioResponse, AudioError> {
        let (response_tx, response_rx) = flume::bounded(1);
        self.command_sender
            .send_async(CommandMessage {
                command,
                response_sender: Some(response_tx),
            })
            .await?;

        response_rx.recv_async().await.map_err(AudioError::from)
    }

    fn send_command_fire_and_forget(&self, command: AudioCommand) -> Result<(), AudioError> {
        self.command_sender
            .try_send(CommandMessage {
                command,
                response_sender: None,
            })
            .map_err(AudioError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_audio_handle_fire_and_forget_success() {
        let (tx, rx) = flume::bounded(10);
        let handle = AudioHandle::new(tx);

        let result = handle.set_volume_immediate(0.5);
        assert!(result.is_ok());

        let msg = rx.try_recv().unwrap();
        assert!(matches!(msg.command, AudioCommand::SetVolume(v) if (v - 0.5).abs() < 0.001));
        assert!(msg.response_sender.is_none());
    }

    #[test_log::test]
    fn test_audio_handle_pause_immediate() {
        let (tx, rx) = flume::bounded(10);
        let handle = AudioHandle::new(tx);

        let result = handle.pause_immediate();
        assert!(result.is_ok());

        let msg = rx.try_recv().unwrap();
        assert!(matches!(msg.command, AudioCommand::Pause));
        assert!(msg.response_sender.is_none());
    }

    #[test_log::test]
    fn test_audio_handle_resume_immediate() {
        let (tx, rx) = flume::bounded(10);
        let handle = AudioHandle::new(tx);

        let result = handle.resume_immediate();
        assert!(result.is_ok());

        let msg = rx.try_recv().unwrap();
        assert!(matches!(msg.command, AudioCommand::Resume));
        assert!(msg.response_sender.is_none());
    }

    #[test_log::test]
    fn test_audio_handle_fire_and_forget_channel_full() {
        let (tx, _rx) = flume::bounded(0);
        let handle = AudioHandle::new(tx);

        let result = handle.set_volume_immediate(0.5);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AudioError::ChannelSend));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_audio_handle_set_volume() {
        let (tx, rx) = flume::bounded(10);
        let handle = AudioHandle::new(tx);

        // Spawn a task to respond
        switchy_async::task::spawn(async move {
            let msg = rx.recv_async().await.unwrap();
            if let Some(resp_tx) = msg.response_sender {
                resp_tx.send_async(AudioResponse::Success).await.unwrap();
            }
        });

        let result = handle.set_volume(0.5).await;
        assert!(result.is_ok());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_audio_handle_pause() {
        let (tx, rx) = flume::bounded(10);
        let handle = AudioHandle::new(tx);

        switchy_async::task::spawn(async move {
            let msg = rx.recv_async().await.unwrap();
            if let Some(resp_tx) = msg.response_sender {
                resp_tx.send_async(AudioResponse::Success).await.unwrap();
            }
        });

        let result = handle.pause().await;
        assert!(result.is_ok());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_audio_handle_resume() {
        let (tx, rx) = flume::bounded(10);
        let handle = AudioHandle::new(tx);

        switchy_async::task::spawn(async move {
            let msg = rx.recv_async().await.unwrap();
            if let Some(resp_tx) = msg.response_sender {
                resp_tx.send_async(AudioResponse::Success).await.unwrap();
            }
        });

        let result = handle.resume().await;
        assert!(result.is_ok());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_audio_handle_seek() {
        let (tx, rx) = flume::bounded(10);
        let handle = AudioHandle::new(tx);

        switchy_async::task::spawn(async move {
            let msg = rx.recv_async().await.unwrap();
            assert!(matches!(msg.command, AudioCommand::Seek(pos) if (pos - 10.5).abs() < 0.001));
            if let Some(resp_tx) = msg.response_sender {
                resp_tx.send_async(AudioResponse::Success).await.unwrap();
            }
        });

        let result = handle.seek(10.5).await;
        assert!(result.is_ok());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_audio_handle_flush() {
        let (tx, rx) = flume::bounded(10);
        let handle = AudioHandle::new(tx);

        switchy_async::task::spawn(async move {
            let msg = rx.recv_async().await.unwrap();
            assert!(matches!(msg.command, AudioCommand::Flush));
            if let Some(resp_tx) = msg.response_sender {
                resp_tx.send_async(AudioResponse::Success).await.unwrap();
            }
        });

        let result = handle.flush().await;
        assert!(result.is_ok());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_audio_handle_reset() {
        let (tx, rx) = flume::bounded(10);
        let handle = AudioHandle::new(tx);

        switchy_async::task::spawn(async move {
            let msg = rx.recv_async().await.unwrap();
            assert!(matches!(msg.command, AudioCommand::Reset));
            if let Some(resp_tx) = msg.response_sender {
                resp_tx.send_async(AudioResponse::Success).await.unwrap();
            }
        });

        let result = handle.reset().await;
        assert!(result.is_ok());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_audio_handle_channel_closed() {
        let (tx, rx) = flume::bounded(10);
        let handle = AudioHandle::new(tx);

        drop(rx);

        let result = handle.set_volume(0.5).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AudioError::ChannelSend));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_audio_handle_response_channel_closed() {
        let (tx, rx) = flume::bounded(10);
        let handle = AudioHandle::new(tx);

        switchy_async::task::spawn(async move {
            let _msg = rx.recv_async().await.unwrap();
            // Don't send response, just drop the response sender
        });

        let result = handle.set_volume(0.5).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AudioError::ChannelReceive));
    }

    #[test_log::test]
    fn test_audio_handle_debug() {
        let (tx, _rx) = flume::bounded(10);
        let handle = AudioHandle::new(tx);

        let debug_str = format!("{handle:?}");
        assert!(debug_str.contains("AudioHandle"));
        assert!(debug_str.contains("command_sender"));
    }

    #[test_log::test]
    fn test_audio_handle_clone() {
        let (tx, rx) = flume::bounded(10);
        let handle = AudioHandle::new(tx);
        let cloned_handle = handle.clone();

        // Both handles should be able to send to the same channel
        handle.pause_immediate().unwrap();
        cloned_handle.resume_immediate().unwrap();

        // Verify both messages were received
        let msg1 = rx.try_recv().unwrap();
        let msg2 = rx.try_recv().unwrap();
        assert!(matches!(msg1.command, AudioCommand::Pause));
        assert!(matches!(msg2.command, AudioCommand::Resume));
    }

    #[test_log::test]
    fn test_audio_command_debug() {
        // Test Debug trait for all AudioCommand variants
        assert!(format!("{:?}", AudioCommand::SetVolume(0.5)).contains("SetVolume"));
        assert!(format!("{:?}", AudioCommand::Pause).contains("Pause"));
        assert!(format!("{:?}", AudioCommand::Resume).contains("Resume"));
        assert!(format!("{:?}", AudioCommand::Seek(10.0)).contains("Seek"));
        assert!(format!("{:?}", AudioCommand::Flush).contains("Flush"));
        assert!(format!("{:?}", AudioCommand::Reset).contains("Reset"));
    }

    #[test_log::test]
    #[allow(clippy::redundant_clone)]
    fn test_audio_command_clone() {
        let cmd = AudioCommand::SetVolume(0.75);
        let cloned = cmd.clone();
        assert!(matches!(cloned, AudioCommand::SetVolume(v) if (v - 0.75).abs() < f64::EPSILON));

        let cmd = AudioCommand::Seek(30.5);
        let cloned = cmd.clone();
        assert!(matches!(cloned, AudioCommand::Seek(v) if (v - 30.5).abs() < f64::EPSILON));
    }

    #[test_log::test]
    fn test_audio_response_debug() {
        let response = AudioResponse::Success;
        assert!(format!("{response:?}").contains("Success"));

        let response = AudioResponse::Error("test error".to_string());
        assert!(format!("{response:?}").contains("Error"));
        assert!(format!("{response:?}").contains("test error"));
    }

    #[test_log::test]
    #[allow(clippy::redundant_clone)]
    fn test_audio_response_clone() {
        let response = AudioResponse::Success;
        let cloned = response.clone();
        assert!(matches!(cloned, AudioResponse::Success));

        let response = AudioResponse::Error("error message".to_string());
        let cloned = response.clone();
        assert!(matches!(cloned, AudioResponse::Error(ref msg) if msg == "error message"));
    }

    #[test_log::test]
    fn test_command_message_debug() {
        let (response_tx, _response_rx) = flume::bounded(1);
        let msg = CommandMessage {
            command: AudioCommand::Pause,
            response_sender: Some(response_tx),
        };

        let debug_str = format!("{msg:?}");
        assert!(debug_str.contains("CommandMessage"));
        assert!(debug_str.contains("command"));
        assert!(debug_str.contains("Pause"));
        assert!(debug_str.contains("response_sender"));
    }

    #[test_log::test]
    fn test_command_message_without_response_sender() {
        let msg = CommandMessage {
            command: AudioCommand::Resume,
            response_sender: None,
        };

        let debug_str = format!("{msg:?}");
        assert!(debug_str.contains("Resume"));
        assert!(debug_str.contains("None"));
    }

    #[test_log::test]
    fn test_audio_error_debug() {
        let err = AudioError::Command("test".to_string());
        assert!(format!("{err:?}").contains("Command"));

        let err = AudioError::ChannelSend;
        assert!(format!("{err:?}").contains("ChannelSend"));

        let err = AudioError::ChannelReceive;
        assert!(format!("{err:?}").contains("ChannelReceive"));

        let err = AudioError::UnexpectedResponse;
        assert!(format!("{err:?}").contains("UnexpectedResponse"));

        let err = AudioError::HandleNotAvailable;
        assert!(format!("{err:?}").contains("HandleNotAvailable"));
    }

    #[test_log::test]
    fn test_audio_error_display() {
        let err = AudioError::Command("test error".to_string());
        assert_eq!(format!("{err}"), "Command error: test error");

        let err = AudioError::ChannelSend;
        assert_eq!(format!("{err}"), "Channel send error");

        let err = AudioError::ChannelReceive;
        assert_eq!(format!("{err}"), "Channel receive error");

        let err = AudioError::UnexpectedResponse;
        assert_eq!(format!("{err}"), "Unexpected response type");

        let err = AudioError::HandleNotAvailable;
        assert_eq!(format!("{err}"), "Handle not available");
    }

    #[test_log::test]
    fn test_audio_error_from_flume_send_error() {
        let (tx, _rx) = flume::bounded::<CommandMessage>(0);
        let msg = CommandMessage {
            command: AudioCommand::Pause,
            response_sender: None,
        };

        // Manually trigger the send error conversion
        let send_err = tx.try_send(msg).unwrap_err();
        let err: AudioError = send_err.into();
        assert!(matches!(err, AudioError::ChannelSend));
    }

    #[test_log::test]
    fn test_audio_error_from_flume_recv_error() {
        let (tx, rx) = flume::bounded::<AudioResponse>(0);
        drop(tx); // Drop sender to cause recv error

        // The recv() returns RecvError which implements Into<AudioError>
        let recv_err = rx.recv().unwrap_err();
        let err: AudioError = recv_err.into();
        assert!(matches!(err, AudioError::ChannelReceive));
    }
}
