package com.moosicbox

import android.net.Uri
import android.os.Looper
import android.util.Log
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.TextureView
import androidx.media3.common.AudioAttributes
import androidx.media3.common.BasePlayer
import androidx.media3.common.C
import androidx.media3.common.DeviceInfo
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.PlaybackException
import androidx.media3.common.PlaybackParameters
import androidx.media3.common.Player
import androidx.media3.common.Timeline
import androidx.media3.common.TrackSelectionParameters
import androidx.media3.common.Tracks
import androidx.media3.common.VideoSize
import androidx.media3.common.text.CueGroup
import androidx.media3.common.util.Clock
import androidx.media3.common.util.ListenerSet
import androidx.media3.common.util.Size
import androidx.media3.common.util.UnstableApi
import androidx.media3.common.util.Util

@UnstableApi
class MoosicBoxPlayer : BasePlayer() {
    private var mediaItems: MutableList<MediaItem> = mutableListOf()
    private var playWhenReady: Boolean = false
    private var playbackState: @Player.State Int = Player.STATE_IDLE
    private var position: Int = C.INDEX_UNSET
    private var positionMs: Long = C.TIME_UNSET
    private var mediaMetadata: MediaMetadata = MediaMetadata.EMPTY
    private var timeline: Timeline = Timeline.EMPTY
    private var volume: Float = 1.0f

    private val listeners: ListenerSet<Player.Listener> =
            ListenerSet(
                    getApplicationLooper(),
                    Clock.DEFAULT,
                    { listener, flags -> listener.onEvents(this, Player.Events(flags)) }
            )

    private val permanentAvailableCommands: Player.Commands =
            Player.Commands.Builder()
                    .addAll(
                            COMMAND_PLAY_PAUSE,
                            COMMAND_PREPARE,
                            COMMAND_STOP,
                            COMMAND_SET_SPEED_AND_PITCH,
                            COMMAND_SET_SHUFFLE_MODE,
                            COMMAND_SET_REPEAT_MODE,
                            COMMAND_GET_CURRENT_MEDIA_ITEM,
                            COMMAND_GET_TIMELINE,
                            COMMAND_GET_METADATA,
                            COMMAND_SET_PLAYLIST_METADATA,
                            COMMAND_SET_MEDIA_ITEM,
                            COMMAND_CHANGE_MEDIA_ITEMS,
                            COMMAND_GET_TRACKS,
                            COMMAND_GET_AUDIO_ATTRIBUTES,
                            COMMAND_SET_AUDIO_ATTRIBUTES,
                            COMMAND_GET_VOLUME,
                            COMMAND_SET_VOLUME,
                            COMMAND_SET_VIDEO_SURFACE,
                            COMMAND_GET_TEXT,
                            COMMAND_RELEASE
                    )
                    .build()

    private val availableCommands: Player.Commands =
            Player.Commands.Builder()
                    .addAll(permanentAvailableCommands)
                    .add(COMMAND_SEEK_TO_DEFAULT_POSITION)
                    .add(COMMAND_SEEK_TO_MEDIA_ITEM)
                    .build()

    init {
        MoosicBoxPlayer.player = this
    }

    override fun getApplicationLooper(): Looper {
        Log.i("MoosicBoxPlayer", "getApplicationLooper")
        return Looper.getMainLooper()
    }

    override fun addListener(listener: Player.Listener) {
        Log.i("MoosicBoxPlayer", "addListener")
        listeners.add(listener)
    }

    override fun removeListener(listener: Player.Listener) {
        Log.i("MoosicBoxPlayer", "removeListener")
        listeners.remove(listener)
    }

    override fun setMediaItems(mediaItems: MutableList<MediaItem>, resetPosition: Boolean) {
        Log.i("MoosicBoxPlayer", "setMediaItems")
        setMediaItems(
                mediaItems,
                if (resetPosition) {
                    C.INDEX_UNSET
                } else {
                    position
                },
                positionMs
        )
    }

    override fun setMediaItems(
            mediaItems: List<MediaItem>,
            startIndex: Int,
            startPositionMs: Long
    ) {
        Log.i("MoosicBoxPlayer", "setMediaItems")
        this.mediaItems = mediaItems.toMutableList()
        this.position = startIndex
        this.positionMs = startPositionMs

        this.timeline = PlaylistTimeline(this.mediaItems)
        this.listeners.queueEvent(Player.EVENT_TIMELINE_CHANGED) { listener ->
            listener.onTimelineChanged(timeline, Player.TIMELINE_CHANGE_REASON_PLAYLIST_CHANGED)
        }

        val mediaItem = this.getCurrentMediaItem()

        if (mediaItem != null) {
            Log.i("MoosicBoxPlayer", "setMediaItems: mediaItem updated to ${mediaItem}")
            this.mediaMetadata = mediaItem.mediaMetadata

            this.listeners.queueEvent(Player.EVENT_MEDIA_METADATA_CHANGED) { listener ->
                listener.onMediaMetadataChanged(this.mediaMetadata)
            }
        }

        this.listeners.flushEvents()
    }

    override fun addMediaItems(index: Int, mediaItems: MutableList<MediaItem>) {
        Log.i("MoosicBoxPlayer", "addMediaItems")
        this.mediaItems.addAll(index, mediaItems)
    }

    override fun moveMediaItems(fromIndex: Int, toIndex: Int, newIndex: Int) {
        Log.i("MoosicBoxPlayer", "moveMediaItems")
        // Actually do it
        val old = this.mediaItems[fromIndex]
        this.mediaItems[fromIndex] = this.mediaItems[newIndex]
        this.mediaItems[newIndex] = old
    }

    override fun replaceMediaItems(
            fromIndex: Int,
            toIndex: Int,
            mediaItems: MutableList<MediaItem>
    ) {
        Log.i("MoosicBoxPlayer", "replaceMediaItems")
        for (x in fromIndex..toIndex) {
            this.mediaItems[x] = mediaItems[x - fromIndex]
        }
    }

    override fun removeMediaItems(fromIndex: Int, toIndex: Int) {
        Log.i("MoosicBoxPlayer", "removeMediaItems")
        for (x in fromIndex..toIndex) {
            this.mediaItems.removeAt(fromIndex)
        }
    }

    override fun getAvailableCommands(): Player.Commands {
        Log.i("MoosicBoxPlayer", "getAvailableCommands")
        return availableCommands
    }

    override fun prepare() {
        Log.i("MoosicBoxPlayer", "prepare")
    }

    override fun getPlaybackState(): Int {
        Log.i("MoosicBoxPlayer", "getPlaybackState")
        return playbackState
    }

    override fun getPlaybackSuppressionReason(): Int {
        Log.i("MoosicBoxPlayer", "getPlaybackSuppressionReason")
        return Player.PLAYBACK_SUPPRESSION_REASON_NONE
    }

    override fun getPlayerError(): PlaybackException? {
        Log.i("MoosicBoxPlayer", "getPlayerError")
        return null
    }

    override fun setPlayWhenReady(playWhenReady: Boolean) {
        Log.i("MoosicBoxPlayer", "setPlayWhenReady playWhenReady=$playWhenReady")
        if (this.playWhenReady != playWhenReady) {
            com.moosicbox.playerplugin.Player.sendMediaEvent(
                    com.moosicbox.playerplugin.MediaEvent(play = playWhenReady)
            )
            setPlayWhenReadyInternal(playWhenReady)
        }
    }

    private fun setPlayWhenReadyInternal(playWhenReady: Boolean, triggerEvents: Boolean = true) {
        Log.i(
                "MoosicBoxPlayer",
                "setPlayWhenReadyInternal playWhenReady=$playWhenReady triggerEvents=$triggerEvents"
        )
        if (this.playWhenReady != playWhenReady) {
            this.playWhenReady = playWhenReady
            if (triggerEvents) {
                this.listeners.queueEvent(Player.EVENT_PLAY_WHEN_READY_CHANGED) { listener ->
                    listener.onPlayWhenReadyChanged(
                            playWhenReady,
                            Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST
                    )
                }
                this.listeners.flushEvents()
            }
        }
    }

    override fun getPlayWhenReady(): Boolean {
        Log.i("MoosicBoxPlayer", "getPlayWhenReady $playWhenReady")
        return playWhenReady
    }

    override fun setRepeatMode(repeatMode: Int) {
        Log.i("MoosicBoxPlayer", "setRepeatMode")
    }

    override fun getRepeatMode(): Int {
        Log.i("MoosicBoxPlayer", "getRepeatMode")
        return Player.REPEAT_MODE_OFF
    }

    override fun setShuffleModeEnabled(shuffleModeEnabled: Boolean) {
        Log.i("MoosicBoxPlayer", "setShuffleModeEnabled")
    }

    override fun getShuffleModeEnabled(): Boolean {
        Log.i("MoosicBoxPlayer", "getShuffleModeEnabled")
        return false
    }

    override fun isLoading(): Boolean {
        Log.i("MoosicBoxPlayer", "isLoading loading=false")
        return false
    }

    override fun seekTo(
            mediaItemIndex: Int,
            positionMs: Long,
            seekCommand: Int,
            isRepeatingCurrentItem: Boolean
    ) {
        if (this.position != mediaItemIndex || this.positionMs != positionMs) {
            Log.i(
                    "MoosicBoxPlayer",
                    "seekTo $mediaItemIndex $positionMs $seekCommand $isRepeatingCurrentItem"
            )
            if (this.position + 1 == mediaItemIndex) {
                com.moosicbox.playerplugin.Player.sendMediaEvent(
                        com.moosicbox.playerplugin.MediaEvent(nextTrack = true)
                )
            } else if (this.position - 1 == mediaItemIndex) {
                com.moosicbox.playerplugin.Player.sendMediaEvent(
                        com.moosicbox.playerplugin.MediaEvent(prevTrack = true)
                )
            }
            seekToInternal(mediaItemIndex, positionMs)
        } else {
            Log.i("MoosicBoxPlayer", "seekTo no change")
        }
    }

    private fun seekToInternal(positionMs: Long, triggerEvents: Boolean = true) {
        Log.i(
                "MoosicBoxPlayer",
                "seekToInternal positionMs=$positionMs triggerEvents=$triggerEvents"
        )
        this.positionMs = positionMs
    }

    private fun seekToInternal(
            mediaItemIndex: Int,
            positionMs: Long,
            triggerEvents: Boolean = true
    ) {
        Log.i(
                "MoosicBoxPlayer",
                "seekToInternal mediaItemIndex=$mediaItemIndex positionMs=$positionMs triggerEvents=$triggerEvents"
        )
        this.position = mediaItemIndex
        this.positionMs = positionMs

        if (triggerEvents) {
            val mediaItem = this.getCurrentMediaItem()

            if (mediaItem != null) {
                this.listeners.queueEvent(Player.EVENT_MEDIA_METADATA_CHANGED) { listener ->
                    listener.onMediaMetadataChanged(mediaItem.mediaMetadata)
                }
            }

            this.listeners.queueEvent(Player.EVENT_TIMELINE_CHANGED) { listener ->
                listener.onTimelineChanged(
                        this.timeline,
                        Player.TIMELINE_CHANGE_REASON_SOURCE_UPDATE
                )
            }
            this.listeners.flushEvents()
        }
    }

    override fun getSeekBackIncrement(): Long {
        Log.i("MoosicBoxPlayer", "getSeekBackIncrement")
        return 0
    }

    override fun getSeekForwardIncrement(): Long {
        Log.i("MoosicBoxPlayer", "getSeekForwardIncrement")
        return 0
    }

    override fun getMaxSeekToPreviousPosition(): Long {
        Log.i("MoosicBoxPlayer", "getMaxSeekToPreviousPosition")
        return 0
    }

    override fun setPlaybackParameters(playbackParameters: PlaybackParameters) {
        Log.i("MoosicBoxPlayer", "setPlaybackParameters")
    }

    override fun getPlaybackParameters(): PlaybackParameters {
        Log.i("MoosicBoxPlayer", "getPlaybackParameters")
        return PlaybackParameters.DEFAULT
    }

    override fun stop() {
        Log.i("MoosicBoxPlayer", "stop")
    }

    private fun stopInternal(triggerEvents: Boolean = true) {
        Log.i("MoosicBoxPlayer", "stop triggerEvents=$triggerEvents")
        setPlayWhenReadyInternal(false, triggerEvents)
    }

    override fun release() {
        Log.i("MoosicBoxPlayer", "release")
    }

    override fun getCurrentTracks(): Tracks {
        Log.i("MoosicBoxPlayer", "getCurrentTracks")
        return Tracks.EMPTY
    }

    override fun getTrackSelectionParameters(): TrackSelectionParameters {
        Log.i("MoosicBoxPlayer", "getTrackSelectionParameters")
        return TrackSelectionParameters.DEFAULT_WITHOUT_CONTEXT
    }

    override fun setTrackSelectionParameters(parameters: TrackSelectionParameters) {
        Log.i("MoosicBoxPlayer", "setTrackSelectionParameters")
    }

    override fun getMediaMetadata(): MediaMetadata {
        Log.i("MoosicBoxPlayer", "getMediaMetadata")
        return getCurrentMediaItem()?.mediaMetadata ?: MediaMetadata.EMPTY
    }

    override fun getPlaylistMetadata(): MediaMetadata {
        Log.i("MoosicBoxPlayer", "getPlaylistMetadata")
        return getCurrentMediaItem()?.mediaMetadata ?: MediaMetadata.EMPTY
    }

    override fun setPlaylistMetadata(mediaMetadata: MediaMetadata) {
        Log.i("MoosicBoxPlayer", "setPlaylistMetadata")
    }

    override fun getCurrentTimeline(): Timeline {
        Log.i("MoosicBoxPlayer", "getCurrentTimeline")
        return timeline
    }

    override fun getCurrentPeriodIndex(): Int {
        Log.i("MoosicBoxPlayer", "getCurrentPeriodIndex")
        return 0
    }

    override fun getCurrentMediaItemIndex(): Int {
        Log.i("MoosicBoxPlayer", "getCurrentMediaItemIndex")
        return position
    }

    override fun getDuration(): Long {
        Log.i("MoosicBoxPlayer", "getDuration")
        return getCurrentMediaItem()?.mediaMetadata?.durationMs ?: 0
    }

    override fun getCurrentPosition(): Long {
        Log.i("MoosicBoxPlayer", "getCurrentPosition")
        return positionMs
    }

    override fun getBufferedPosition(): Long {
        Log.i("MoosicBoxPlayer", "getBufferedPosition")
        return positionMs
    }

    override fun getTotalBufferedDuration(): Long {
        Log.i("MoosicBoxPlayer", "getTotalBufferedDuration")
        return getCurrentMediaItem()?.mediaMetadata?.durationMs ?: 0
    }

    override fun isPlayingAd(): Boolean {
        Log.i("MoosicBoxPlayer", "isPlayingAd")
        return false
    }

    override fun getCurrentAdGroupIndex(): Int {
        Log.i("MoosicBoxPlayer", "getCurrentAdGroupIndex")
        return -1
    }

    override fun getCurrentAdIndexInAdGroup(): Int {
        Log.i("MoosicBoxPlayer", "getCurrentAdIndexInAdGroup")
        return -1
    }

    override fun getContentPosition(): Long {
        Log.i("MoosicBoxPlayer", "getContentPosition")
        return positionMs
    }

    override fun getContentBufferedPosition(): Long {
        Log.i("MoosicBoxPlayer", "getContentBufferedPosition")
        return positionMs
    }

    override fun getAudioAttributes(): AudioAttributes {
        Log.i("MoosicBoxPlayer", "getAudioAttributes")
        return AudioAttributes.DEFAULT
    }

    override fun setVolume(volume: Float) {
        Log.i("MoosicBoxPlayer", "setVolume")
    }

    override fun getVolume(): Float {
        Log.i("MoosicBoxPlayer", "getVolume")
        return 1.0f
    }

    override fun clearVideoSurface() {
        Log.i("MoosicBoxPlayer", "clearVideoSurface")
    }

    override fun clearVideoSurface(surface: Surface?) {
        Log.i("MoosicBoxPlayer", "clearVideoSurface")
    }

    override fun setVideoSurface(surface: Surface?) {
        Log.i("MoosicBoxPlayer", "setVideoSurface")
    }

    override fun setVideoSurfaceHolder(surfaceHolder: SurfaceHolder?) {
        Log.i("MoosicBoxPlayer", "setVideoSurfaceHolder")
    }

    override fun clearVideoSurfaceHolder(surfaceHolder: SurfaceHolder?) {
        Log.i("MoosicBoxPlayer", "clearVideoSurfaceHolder")
    }

    override fun setVideoSurfaceView(surfaceView: SurfaceView?) {
        Log.i("MoosicBoxPlayer", "setVideoSurfaceView")
    }

    override fun clearVideoSurfaceView(surfaceView: SurfaceView?) {
        Log.i("MoosicBoxPlayer", "clearVideoSurfaceView")
    }

    override fun setVideoTextureView(textureView: TextureView?) {
        Log.i("MoosicBoxPlayer", "setVideoTextureView")
    }

    override fun clearVideoTextureView(textureView: TextureView?) {
        Log.i("MoosicBoxPlayer", "clearVideoTextureView")
    }

    override fun getVideoSize(): VideoSize {
        Log.i("MoosicBoxPlayer", "getVideoSize")
        return VideoSize.UNKNOWN
    }

    override fun getSurfaceSize(): Size {
        Log.i("MoosicBoxPlayer", "getSurfaceSize")
        return Size.ZERO
    }

    override fun getCurrentCues(): CueGroup {
        Log.i("MoosicBoxPlayer", "getCurrentCues")
        return CueGroup.EMPTY_TIME_ZERO
    }

    override fun getDeviceInfo(): DeviceInfo {
        Log.i("MoosicBoxPlayer", "getDeviceInfo")
        return DeviceInfo.UNKNOWN
    }

    override fun getDeviceVolume(): Int {
        Log.i("MoosicBoxPlayer", "getDeviceVolume")
        return 1
    }

    override fun isDeviceMuted(): Boolean {
        Log.i("MoosicBoxPlayer", "isDeviceMuted")
        return false
    }

    override fun setDeviceVolume(volume: Int) {
        Log.i("MoosicBoxPlayer", "setDeviceVolume")
    }

    override fun setDeviceVolume(volume: Int, flags: Int) {
        Log.i("MoosicBoxPlayer", "setDeviceVolume")
    }

    override fun increaseDeviceVolume() {
        Log.i("MoosicBoxPlayer", "increaseDeviceVolume")
    }

    override fun increaseDeviceVolume(flags: Int) {
        Log.i("MoosicBoxPlayer", "increaseDeviceVolume")
    }

    override fun decreaseDeviceVolume() {
        Log.i("MoosicBoxPlayer", "decreaseDeviceVolume")
    }

    override fun decreaseDeviceVolume(flags: Int) {
        Log.i("MoosicBoxPlayer", "decreaseDeviceVolume")
    }

    override fun setDeviceMuted(muted: Boolean) {
        Log.i("MoosicBoxPlayer", "setDeviceMuted")
    }

    override fun setDeviceMuted(muted: Boolean, flags: Int) {
        Log.i("MoosicBoxPlayer", "setDeviceMuted")
    }

    override fun setAudioAttributes(audioAttributes: AudioAttributes, handleAudioFocus: Boolean) {
        Log.i("MoosicBoxPlayer", "setAudioAttributes")
    }

    private fun seekToPositionInternal(position: Int) {
        seekToInternal(position, this.positionMs)
    }

    private fun setVolumeInternal(volume: Float, triggerEvents: Boolean = true) {
        Log.i("MoosicBoxPlayer", "setVolumeInternal volume=$volume triggerEvents=$triggerEvents")
        if (this.volume != volume) {
            this.volume = volume
            if (triggerEvents) {
                this.listeners.queueEvent(Player.EVENT_VOLUME_CHANGED) { listener ->
                    listener.onVolumeChanged(volume)
                }
                this.listeners.flushEvents()
            }
        }
    }

    private fun setPlaybackStateInternal(
            playbackState: @Player.State Int,
            triggerEvents: Boolean = true
    ) {
        Log.i(
                "MoosicBoxPlayer",
                "setPlaybackStateInternal playbackState=$playbackState triggerEvents=$triggerEvents"
        )
        if (this.playbackState != playbackState) {
            this.playbackState = playbackState
            if (triggerEvents) {
                this.listeners.queueEvent(Player.EVENT_PLAYBACK_STATE_CHANGED) { listener ->
                    listener.onPlaybackStateChanged(playbackState)
                }
                this.listeners.flushEvents()
            }
        }
    }

    companion object {
        lateinit var player: MoosicBoxPlayer

        init {
            com.moosicbox.playerplugin.Player.updateState = { state ->
                Log.i("MoosicBoxPlayer", "Received state $state")

                var mediaItems: List<MediaItem>? = null

                state.playlist?.also { playlist ->
                    mediaItems =
                            playlist.tracks.map { track ->
                                var metadataBuilder =
                                        MediaMetadata.Builder()
                                                .setArtist(track.artist)
                                                .setAlbumTitle(track.album)
                                                .setTitle(track.title)
                                                .setDurationMs((track.duration * 1000).toLong())
                                                .setTrackNumber(track.number)

                                track.albumCover?.also {
                                    metadataBuilder = metadataBuilder.setArtworkUri(Uri.parse(it))
                                }

                                val metadata = metadataBuilder.build()

                                MediaItem.Builder()
                                        .setMediaId("media-${track.id}")
                                        .setMediaMetadata(metadata)
                                        .build()
                            }

                    Log.i("MoosicBoxPlayer", "updateState mediaItems=$mediaItems")
                }

                if (mediaItems != null && mediaItems!!.isEmpty()) {
                    player.setPlaybackStateInternal(Player.STATE_IDLE)

                    player.setMediaItems(mediaItems!!)
                } else {
                    player.setPlaybackStateInternal(Player.STATE_READY)
                    if (mediaItems != null) {
                        if (state.position != null) {
                            if (state.seek != null) {
                                player.setMediaItems(
                                        mediaItems!!,
                                        state.position!!.toInt(),
                                        (state.seek!! * 1000).toLong()
                                )
                            } else {
                                player.setMediaItems(mediaItems!!, state.position!!.toInt(), 0)
                            }
                        } else {
                            player.setMediaItems(mediaItems!!)
                        }
                    } else if (state.position != null) {
                        if (state.seek != null) {
                            player.seekToInternal(
                                    state.position!!.toInt(),
                                    (state.seek!! * 1000).toLong()
                            )
                        } else {
                            player.seekToPositionInternal(state.position!!.toInt())
                        }
                    } else if (state.seek != null) {
                        player.seekToInternal((state.seek!! * 1000).toLong())
                    }
                    state.volume?.also { player.setVolumeInternal(it.toFloat()) }
                    state.playing?.also { player.setPlayWhenReadyInternal(it) }
                    player.prepare()
                }
            }
        }
    }

    internal class PlaylistTimeline
    @JvmOverloads
    constructor(
            mediaItems: List<MediaItem>,
            shuffledIndices: IntArray = createUnshuffledIndices(mediaItems.size)
    ) : Timeline() {
        private val mediaItems: List<MediaItem>
        private val shuffledIndices: IntArray
        private val indicesInShuffled: IntArray

        init {
            this.mediaItems = mediaItems
            this.shuffledIndices = shuffledIndices.copyOf(shuffledIndices.size)
            indicesInShuffled = IntArray(shuffledIndices.size)
            for (i in shuffledIndices.indices) {
                indicesInShuffled[shuffledIndices[i]] = i
            }
        }

        override fun getWindowCount(): Int {
            return mediaItems.size
        }

        override fun getWindow(
                windowIndex: Int,
                window: Window,
                defaultPositionProjectionUs: Long
        ): Window {
            window[
                    0,
                    mediaItems[windowIndex],
                    null,
                    0,
                    0,
                    0,
                    true,
                    false,
                    null,
                    0,
                    Util.msToUs(DEFAULT_DURATION_MS),
                    windowIndex,
                    windowIndex] = 0
            window.isPlaceholder = false
            return window
        }

        override fun getNextWindowIndex(
                windowIndex: Int,
                repeatMode: @Player.RepeatMode Int,
                shuffleModeEnabled: Boolean
        ): Int {
            if (repeatMode == REPEAT_MODE_ONE) {
                return windowIndex
            }
            if (windowIndex == getLastWindowIndex(shuffleModeEnabled)) {
                return if (repeatMode == REPEAT_MODE_ALL) getFirstWindowIndex(shuffleModeEnabled)
                else C.INDEX_UNSET
            }
            return if (shuffleModeEnabled) shuffledIndices[indicesInShuffled[windowIndex] + 1]
            else windowIndex + 1
        }

        override fun getPreviousWindowIndex(
                windowIndex: Int,
                repeatMode: @Player.RepeatMode Int,
                shuffleModeEnabled: Boolean
        ): Int {
            if (repeatMode == REPEAT_MODE_ONE) {
                return windowIndex
            }
            if (windowIndex == getFirstWindowIndex(shuffleModeEnabled)) {
                return if (repeatMode == REPEAT_MODE_ALL) getLastWindowIndex(shuffleModeEnabled)
                else C.INDEX_UNSET
            }
            return if (shuffleModeEnabled) shuffledIndices[indicesInShuffled[windowIndex] - 1]
            else windowIndex - 1
        }

        override fun getLastWindowIndex(shuffleModeEnabled: Boolean): Int {
            if (isEmpty) {
                return C.INDEX_UNSET
            }
            return if (shuffleModeEnabled) shuffledIndices[windowCount - 1] else windowCount - 1
        }

        override fun getFirstWindowIndex(shuffleModeEnabled: Boolean): Int {
            if (isEmpty) {
                return C.INDEX_UNSET
            }
            return if (shuffleModeEnabled) shuffledIndices[0] else 0
        }

        override fun getPeriodCount(): Int {
            return windowCount
        }

        override fun getPeriod(periodIndex: Int, period: Period, setIds: Boolean): Period {
            period[null, null, periodIndex, Util.msToUs(DEFAULT_DURATION_MS)] = 0
            return period
        }

        override fun getIndexOfPeriod(uid: Any): Int {
            throw UnsupportedOperationException()
        }

        override fun getUidOfPeriod(periodIndex: Int): Any {
            throw UnsupportedOperationException()
        }

        companion object {
            private const val DEFAULT_DURATION_MS: Long = 100

            private fun createUnshuffledIndices(length: Int): IntArray {
                val indices = IntArray(length)
                for (i in 0 until length) {
                    indices[i] = i
                }
                return indices
            }
        }
    }
}
