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
    SetVolume(f64),
    Pause,
    Resume,
    Seek(f64),
    Flush,
    Reset,
}

/// Response returned after executing an audio command.
#[derive(Debug, Clone)]
pub enum AudioResponse {
    Success,
    Error(String),
}

/// Message structure for sending commands through channels.
#[derive(Debug)]
pub struct CommandMessage {
    pub command: AudioCommand,
    pub response_sender: Option<flume::Sender<AudioResponse>>,
}

/// Errors that can occur during audio command operations.
#[derive(Debug, Error)]
pub enum AudioError {
    #[error("Command error: {0}")]
    Command(String),
    #[error("Channel send error")]
    ChannelSend,
    #[error("Channel receive error")]
    ChannelReceive,
    #[error("Unexpected response type")]
    UnexpectedResponse,
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
