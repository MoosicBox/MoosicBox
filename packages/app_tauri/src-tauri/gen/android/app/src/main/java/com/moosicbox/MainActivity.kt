package com.moosicbox

import android.content.ComponentName
import android.net.Uri
import android.util.Log
import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.DeviceInfo
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Metadata
import androidx.media3.common.PlaybackException
import androidx.media3.common.PlaybackParameters
import androidx.media3.common.Player
import androidx.media3.common.Timeline
import androidx.media3.common.TrackSelectionParameters
import androidx.media3.common.Tracks
import androidx.media3.common.VideoSize
import androidx.media3.common.text.CueGroup
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import com.google.common.util.concurrent.MoreExecutors

class MainActivity : TauriActivity() {
    @OptIn(UnstableApi::class)
    override fun onStart() {
        val sessionToken = SessionToken(this, ComponentName(this, PlaybackService::class.java))
        val controllerFuture = MediaController.Builder(this, sessionToken).buildAsync()
        controllerFuture.addListener(
                {
                    // Call controllerFuture.get() to retrieve the MediaController.
                    // MediaController implements the Player interface, so it can be
                    // attached to the PlayerView UI component.
                    val player = controllerFuture.get()
                    player.addListener(PlayerListener())
                },
                MoreExecutors.directExecutor()
        )
        super.onStart()
    }

    @UnstableApi
    private inner class PlayerListener : Player.Listener {
        override fun onEvents(player: Player, events: Player.Events) {
            Log.i("MOOSICBOX: Player.Listener", "onEvents $player $events")
            super.onEvents(player, events)
        }

        override fun onAudioAttributesChanged(audioAttributes: AudioAttributes) {
            Log.i("MOOSICBOX: Player.Listener", "onAudioAttributesChanged $audioAttributes")
            super.onAudioAttributesChanged(audioAttributes)
        }

        override fun onAvailableCommandsChanged(availableCommands: Player.Commands) {
            Log.i("MOOSICBOX: Player.Listener", "onAvailableCommandsChanged $availableCommands")
            super.onAvailableCommandsChanged(availableCommands)
        }

        override fun onAudioSessionIdChanged(audioSessionId: Int) {
            Log.i("MOOSICBOX: Player.Listener", "onAudioSessionIdChanged $audioSessionId")
            super.onAudioSessionIdChanged(audioSessionId)
        }

        override fun onCues(cueGroup: CueGroup) {
            Log.i("MOOSICBOX: Player.Listener", "onCues $cueGroup")
            super.onCues(cueGroup)
        }

        override fun onDeviceInfoChanged(deviceInfo: DeviceInfo) {
            Log.i("MOOSICBOX: Player.Listener", "onDeviceInfoChanged $deviceInfo")
            super.onDeviceInfoChanged(deviceInfo)
        }

        override fun onDeviceVolumeChanged(volume: Int, muted: Boolean) {
            Log.i("MOOSICBOX: Player.Listener", "onDeviceVolumeChanged $volume $muted")
            super.onDeviceVolumeChanged(volume, muted)
        }

        override fun onIsLoadingChanged(isLoading: Boolean) {
            Log.i("MOOSICBOX: Player.Listener", "onIsLoadingChanged $isLoading")
            super.onIsLoadingChanged(isLoading)
        }

        override fun onIsPlayingChanged(isPlaying: Boolean) {
            Log.i("MOOSICBOX: Player.Listener", "onIsPlayingChanged $isPlaying")
            super.onIsPlayingChanged(isPlaying)
        }

        override fun onMaxSeekToPreviousPositionChanged(maxSeekToPreviousPositionMs: Long) {
            Log.i(
                    "Player.Listener",
                    "onMaxSeekToPreviousPositionChanged $maxSeekToPreviousPositionMs"
            )
            super.onMaxSeekToPreviousPositionChanged(maxSeekToPreviousPositionMs)
        }

        override fun onMediaItemTransition(mediaItem: MediaItem?, reason: Int) {
            Log.i("MOOSICBOX: Player.Listener", "onMediaItemTransition $mediaItem $reason")
            super.onMediaItemTransition(mediaItem, reason)
        }

        override fun onMediaMetadataChanged(mediaMetadata: MediaMetadata) {
            Log.i("MOOSICBOX: Player.Listener", "onMediaMetadataChanged $mediaMetadata")
            super.onMediaMetadataChanged(mediaMetadata)
        }

        override fun onMetadata(metadata: Metadata) {
            Log.i("MOOSICBOX: Player.Listener", "onMetadata $metadata")
            super.onMetadata(metadata)
        }

        override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
            Log.i("MOOSICBOX: Player.Listener", "onPlayWhenReadyChanged $playWhenReady $reason")
            super.onPlayWhenReadyChanged(playWhenReady, reason)
        }

        override fun onPlaybackParametersChanged(playbackParameters: PlaybackParameters) {
            Log.i("MOOSICBOX: Player.Listener", "onPlaybackParametersChanged $playbackParameters")
            super.onPlaybackParametersChanged(playbackParameters)
        }

        override fun onPlaybackStateChanged(playbackState: Int) {
            Log.i("MOOSICBOX: Player.Listener", "onPlaybackStateChanged $playbackState")
            super.onPlaybackStateChanged(playbackState)
        }

        override fun onPlaybackSuppressionReasonChanged(playbackSuppressionReason: Int) {
            Log.i(
                    "Player.Listener",
                    "onPlaybackSuppressionReasonChanged $playbackSuppressionReason"
            )
            super.onPlaybackSuppressionReasonChanged(playbackSuppressionReason)
        }

        override fun onPlayerError(error: PlaybackException) {
            Log.i("MOOSICBOX: Player.Listener", "onPlayerError $error")
            super.onPlayerError(error)
        }

        override fun onPlayerErrorChanged(error: PlaybackException?) {
            Log.i("MOOSICBOX: Player.Listener", "onPlayerErrorChanged $error")
            super.onPlayerErrorChanged(error)
        }

        override fun onPlaylistMetadataChanged(mediaMetadata: MediaMetadata) {
            Log.i("MOOSICBOX: Player.Listener", "onPlaylistMetadataChanged $mediaMetadata")
            super.onPlaylistMetadataChanged(mediaMetadata)
        }

        override fun onPositionDiscontinuity(
                oldPosition: Player.PositionInfo,
                newPosition: Player.PositionInfo,
                reason: Int
        ) {
            Log.i(
                    "MOOSICBOX: Player.Listener",
                    "onPositionDiscontinuity $oldPosition $newPosition $reason"
            )
            super.onPositionDiscontinuity(oldPosition, newPosition, reason)
        }

        override fun onRenderedFirstFrame() {
            Log.i("MOOSICBOX: Player.Listener", "onRenderedFirstFrame")
            super.onRenderedFirstFrame()
        }

        override fun onRepeatModeChanged(repeatMode: Int) {
            Log.i("MOOSICBOX: Player.Listener", "onRepeatModeChanged $repeatMode")
            super.onRepeatModeChanged(repeatMode)
        }

        override fun onSeekBackIncrementChanged(seekBackIncrementMs: Long) {
            Log.i("MOOSICBOX: Player.Listener", "onSeekBackIncrementChanged $seekBackIncrementMs")
            super.onSeekBackIncrementChanged(seekBackIncrementMs)
        }

        override fun onSeekForwardIncrementChanged(seekForwardIncrementMs: Long) {
            Log.i(
                    "MOOSICBOX: Player.Listener",
                    "onSeekForwardIncrementChanged $seekForwardIncrementMs"
            )
            super.onSeekForwardIncrementChanged(seekForwardIncrementMs)
        }

        override fun onShuffleModeEnabledChanged(shuffleModeEnabled: Boolean) {
            Log.i("MOOSICBOX: Player.Listener", "onShuffleModeEnabledChanged $shuffleModeEnabled")
            super.onShuffleModeEnabledChanged(shuffleModeEnabled)
        }

        override fun onSkipSilenceEnabledChanged(skipSilenceEnabled: Boolean) {
            Log.i("MOOSICBOX: Player.Listener", "onSkipSilenceEnabledChanged $skipSilenceEnabled")
            super.onSkipSilenceEnabledChanged(skipSilenceEnabled)
        }

        override fun onSurfaceSizeChanged(width: Int, height: Int) {
            Log.i("MOOSICBOX: Player.Listener", "onSurfaceSizeChanged $width $height")
            super.onSurfaceSizeChanged(width, height)
        }

        override fun onTimelineChanged(timeline: Timeline, reason: Int) {
            Log.i("MOOSICBOX: Player.Listener", "onTimelineChanged $reason")
            super.onTimelineChanged(timeline, reason)
        }

        override fun onTrackSelectionParametersChanged(parameters: TrackSelectionParameters) {
            Log.i("MOOSICBOX: Player.Listener", "onTrackSelectionParametersChanged $parameters")
            super.onTrackSelectionParametersChanged(parameters)
        }

        override fun onTracksChanged(tracks: Tracks) {
            Log.i("MOOSICBOX: Player.Listener", "onTracksChanged $tracks")
            super.onTracksChanged(tracks)
        }

        override fun onVideoSizeChanged(videoSize: VideoSize) {
            Log.i("MOOSICBOX: Player.Listener", "onVideoSizeChanged $videoSize")
            super.onVideoSizeChanged(videoSize)
        }

        override fun onVolumeChanged(volume: Float) {
            Log.i("MOOSICBOX: Player.Listener", "onVolumeChanged $volume")
            super.onVolumeChanged(volume)
        }
    }
}
