//! Progress tracking for audio playback.
//!
//! This module provides [`ProgressTracker`], a reusable component for tracking audio playback
//! progress and position. It can be used by any audio output implementation to monitor consumed
//! samples and trigger callbacks when the playback position changes significantly.

use atomic_float::AtomicF64;
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU32, AtomicUsize, Ordering},
};

/// A reusable progress tracker for audio output implementations.
///
/// This struct encapsulates all the logic needed to track audio playback progress
/// and can be used by any `AudioOutput` implementation (CPAL, `PulseAudio`, etc.).
pub struct ProgressTracker {
    /// Counter for consumed audio samples
    consumed_samples: Arc<AtomicUsize>,
    /// Audio sample rate in Hz
    sample_rate: Arc<AtomicU32>,
    /// Number of audio channels
    channels: Arc<AtomicU32>,
    /// Progress callback function that gets called when position changes significantly
    #[allow(clippy::type_complexity)]
    callback: Arc<RwLock<Option<Box<dyn Fn(f64) + Send + Sync + 'static>>>>,
    /// Last reported position to avoid excessive callbacks
    last_reported_position: Arc<AtomicF64>,
    /// Minimum position change (in seconds) before calling the callback
    threshold: f64,
}

impl ProgressTracker {
    /// Create a new `ProgressTracker` with the specified threshold.
    ///
    /// # Arguments
    /// * `threshold` - Minimum position change in seconds before calling the progress callback (default: 0.1)
    #[must_use]
    pub fn new(threshold: Option<f64>) -> Self {
        Self {
            consumed_samples: Arc::new(AtomicUsize::new(0)),
            sample_rate: Arc::new(AtomicU32::new(0)),
            channels: Arc::new(AtomicU32::new(0)),
            callback: Arc::new(RwLock::new(None)),
            last_reported_position: Arc::new(AtomicF64::new(0.0)),
            threshold: threshold.unwrap_or(0.1),
        }
    }

    /// Set the audio specification (sample rate and channels).
    ///
    /// This should be called when the audio format is known.
    pub fn set_audio_spec(&self, sample_rate: u32, channels: u32) {
        self.sample_rate.store(sample_rate, Ordering::SeqCst);
        self.channels.store(channels, Ordering::SeqCst);
        log::debug!("ProgressTracker: audio spec set - rate={sample_rate}, channels={channels}");
    }

    /// Set the progress callback function.
    ///
    /// The callback will be called whenever the playback position changes by more than the threshold.
    pub fn set_callback(&self, callback: Option<Box<dyn Fn(f64) + Send + Sync + 'static>>) {
        if let Ok(mut cb) = self.callback.write() {
            *cb = callback;
            log::debug!("ProgressTracker: callback set");
        } else {
            log::error!("ProgressTracker: failed to acquire write lock for callback");
        }
    }

    /// Get a reference to the consumed samples counter.
    ///
    /// This can be passed to `AudioOutput` implementations that need to track consumed samples.
    #[must_use]
    pub fn consumed_samples_counter(&self) -> Arc<AtomicUsize> {
        self.consumed_samples.clone()
    }

    /// Update the consumed samples count and potentially trigger the progress callback.
    ///
    /// This method should be called by `AudioOutput` implementations when samples are consumed.
    /// It will automatically calculate the current position and call the progress callback
    /// if the position has changed by more than the threshold.
    ///
    /// # Arguments
    /// * `additional_samples` - Number of additional samples that were consumed
    pub fn update_consumed_samples(&self, additional_samples: usize) {
        if additional_samples == 0 {
            return;
        }

        let new_consumed = self
            .consumed_samples
            .fetch_add(additional_samples, Ordering::SeqCst)
            + additional_samples;
        let sample_rate = self.sample_rate.load(Ordering::SeqCst);
        let channels = self.channels.load(Ordering::SeqCst);

        if sample_rate > 0 && channels > 0 {
            #[allow(clippy::cast_precision_loss)]
            let current_position =
                new_consumed as f64 / (f64::from(sample_rate) * f64::from(channels));
            let last_position = self.last_reported_position.load(Ordering::SeqCst);

            // Only call callback if position has changed significantly
            if (current_position - last_position).abs() > self.threshold {
                self.last_reported_position
                    .store(current_position, Ordering::SeqCst);

                // Call progress callback if it exists
                if let Ok(callback_guard) = self.callback.try_read()
                    && let Some(callback) = callback_guard.as_ref()
                {
                    callback(current_position);
                }
            }
        }
    }

    /// Get the current playback position in seconds.
    ///
    /// Returns `None` if audio spec hasn't been set yet.
    #[must_use]
    pub fn get_position(&self) -> Option<f64> {
        let consumed = self.consumed_samples.load(Ordering::SeqCst);
        let sample_rate = self.sample_rate.load(Ordering::SeqCst);
        let channels = self.channels.load(Ordering::SeqCst);

        if sample_rate > 0 && channels > 0 {
            #[allow(clippy::cast_precision_loss)]
            Some(consumed as f64 / (f64::from(sample_rate) * f64::from(channels)))
        } else {
            None
        }
    }

    /// Set the consumed samples count to a specific value.
    ///
    /// This is useful for seeking operations where the position needs to be set to a specific value.
    pub fn set_consumed_samples(&self, samples: usize) {
        self.consumed_samples.store(samples, Ordering::SeqCst);

        // Update last reported position to avoid immediate callback
        if let Some(position) = self.get_position() {
            self.last_reported_position
                .store(position, Ordering::SeqCst);
        }

        log::debug!("ProgressTracker: consumed samples set to {samples}");
    }

    /// Reset the progress tracker for a new track.
    ///
    /// This clears the consumed samples count and last reported position.
    pub fn reset(&self) {
        self.consumed_samples.store(0, Ordering::SeqCst);
        self.last_reported_position.store(0.0, Ordering::SeqCst);
        log::debug!("ProgressTracker: reset for new track");
    }

    /// Get clones of the internal atomic references for use in audio callbacks.
    ///
    /// This is useful for audio implementations that need to access the tracking state
    /// from within audio callback functions where `&self` is not available.
    ///
    /// Returns: (`consumed_samples`, `sample_rate`, channels, callback, `last_reported_position`)
    #[must_use]
    #[allow(clippy::type_complexity)]
    pub fn get_callback_refs(
        &self,
    ) -> (
        Arc<AtomicUsize>,
        Arc<AtomicU32>,
        Arc<AtomicU32>,
        Arc<RwLock<Option<Box<dyn Fn(f64) + Send + Sync + 'static>>>>,
        Arc<AtomicF64>,
    ) {
        (
            self.consumed_samples.clone(),
            self.sample_rate.clone(),
            self.channels.clone(),
            self.callback.clone(),
            self.last_reported_position.clone(),
        )
    }

    /// Update consumed samples and check for progress callback (optimized for audio callbacks).
    ///
    /// This is a convenience method that can be used directly in audio callbacks
    /// using the references obtained from `get_callback_refs()`.
    #[allow(clippy::type_complexity)]
    pub fn update_from_callback_refs(
        consumed_samples: &Arc<AtomicUsize>,
        sample_rate: &Arc<AtomicU32>,
        channels: &Arc<AtomicU32>,
        callback: &Arc<RwLock<Option<Box<dyn Fn(f64) + Send + Sync + 'static>>>>,
        last_reported_position: &Arc<AtomicF64>,
        additional_samples: usize,
        threshold: f64,
    ) {
        if additional_samples == 0 {
            return;
        }

        let new_consumed =
            consumed_samples.fetch_add(additional_samples, Ordering::SeqCst) + additional_samples;
        let sample_rate_val = sample_rate.load(Ordering::SeqCst);
        let channels_val = channels.load(Ordering::SeqCst);

        if sample_rate_val > 0 && channels_val > 0 {
            #[allow(clippy::cast_precision_loss)]
            let current_position =
                new_consumed as f64 / (f64::from(sample_rate_val) * f64::from(channels_val));
            let last_position = last_reported_position.load(Ordering::SeqCst);

            // Only call callback if position has changed significantly
            if (current_position - last_position).abs() > threshold {
                last_reported_position.store(current_position, Ordering::SeqCst);

                // Call progress callback if it exists
                if let Ok(callback_guard) = callback.try_read()
                    && let Some(cb) = callback_guard.as_ref()
                {
                    cb(current_position);
                }
            }
        }
    }
}

impl std::fmt::Debug for ProgressTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgressTracker")
            .field(
                "consumed_samples",
                &self.consumed_samples.load(Ordering::SeqCst),
            )
            .field("sample_rate", &self.sample_rate.load(Ordering::SeqCst))
            .field("channels", &self.channels.load(Ordering::SeqCst))
            .field(
                "last_reported_position",
                &self.last_reported_position.load(Ordering::SeqCst),
            )
            .field("threshold", &self.threshold)
            .finish_non_exhaustive()
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new(None)
    }
}
