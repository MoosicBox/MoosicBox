package com.moosicbox

import android.app.Notification
import android.app.PendingIntent
import android.content.Intent
import android.graphics.Bitmap
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.os.Build
import android.util.Log
import android.view.KeyEvent
import androidx.annotation.OptIn
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.MediaLibraryService
import androidx.media3.session.MediaSession
import androidx.media3.ui.PlayerNotificationManager
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.SettableFuture
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

class PlaybackService : MediaLibraryService() {
    private lateinit var mediaLibrarySession: MediaLibrarySession
    private lateinit var audioTrack: AudioTrack
    private lateinit var player: Player
    private lateinit var notificationManager: PlayerNotificationManager

    init {
        PlaybackService.instance = this
    }

    override fun onCreate() {
        super.onCreate()
        val player = MoosicBoxPlayer()
        this.player = player
        mediaLibrarySession =
                MediaLibrarySession.Builder(this, player, MediaLibrarySessionCallback()).build()

        audioTrack =
                AudioTrack.Builder()
                        .setAudioAttributes(
                                AudioAttributes.Builder()
                                        .setLegacyStreamType(AudioManager.STREAM_MUSIC)
                                        .build()
                        )
                        .setAudioFormat(
                                AudioFormat.Builder()
                                        .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                                        .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO)
                                        .setSampleRate(48000)
                                        .build()
                        )
                        .setBufferSizeInBytes(
                                AudioTrack.getMinBufferSize(
                                        48000,
                                        AudioFormat.CHANNEL_OUT_STEREO,
                                        AudioFormat.ENCODING_PCM_16BIT
                                )
                        )
                        .setTransferMode(AudioTrack.MODE_STREAM)
                        .build()
    }

    // Remember to release the player and media session in onDestroy
    override fun onDestroy() {
        audioTrack.stop()
        audioTrack.release()

        if (player.isPlaying) {
            player.stop()
        }
        notificationManager.setPlayer(null)
        player.release()
        stopSelf()

        mediaLibrarySession.run {
            player.release()
            release()
        }

        super.onDestroy()
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaLibrarySession? {
        return mediaLibrarySession
    }

    private val notificationListener =
            @OptIn(UnstableApi::class)
            object : PlayerNotificationManager.NotificationListener {
                override fun onNotificationCancelled(
                        notificationId: Int,
                        dismissedByUser: Boolean
                ) {
                    super.onNotificationCancelled(notificationId, dismissedByUser)
                    if (player.isPlaying) {
                        player.stop()
                        player.release()
                    }
                }

                override fun onNotificationPosted(
                        notificationId: Int,
                        notification: Notification,
                        ongoing: Boolean
                ) {
                    super.onNotificationPosted(notificationId, notification, ongoing)
                }
            }

    private val audioDescriptor =
            @OptIn(UnstableApi::class)
            object : PlayerNotificationManager.MediaDescriptionAdapter {
                override fun getCurrentContentTitle(player: Player): CharSequence {
                    return player.currentMediaItem?.mediaMetadata?.albumTitle!!
                }

                override fun createCurrentContentIntent(player: Player): PendingIntent? {
                    return pendingIntent()
                }

                override fun getCurrentContentText(player: Player): CharSequence? {
                    return ""
                }

                override fun getCurrentLargeIcon(
                        player: Player,
                        callback: PlayerNotificationManager.BitmapCallback
                ): Bitmap? {
                    return null
                }
            }

    private fun pendingIntent(): PendingIntent? {
        val intent = Intent(applicationContext, PlaybackService::class.java)
        return PendingIntent.getActivity(
                applicationContext,
                0,
                intent,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
    }

    @UnstableApi
    private inner class MediaLibrarySessionCallback : MediaLibrarySession.Callback {
        override fun onConnect(
                session: MediaSession,
                controller: MediaSession.ControllerInfo
        ): MediaSession.ConnectionResult {
            audioTrack.play()

            // Set available player and session commands.
            return MediaSession.ConnectionResult.AcceptedResultBuilder(session)
                    .setAvailablePlayerCommands(
                            MediaSession.ConnectionResult.DEFAULT_PLAYER_COMMANDS
                                    .buildUpon()
                                    .build()
                    )
                    .setAvailableSessionCommands(
                            MediaSession.ConnectionResult.DEFAULT_SESSION_COMMANDS
                                    .buildUpon()
                                    .build()
                    )
                    .build()
        }

        override fun onMediaButtonEvent(
                session: MediaSession,
                controllerInfo: MediaSession.ControllerInfo,
                intent: Intent
        ): Boolean {
            Log.i("PlaybackService", "onMediaButtonEvent $session $controllerInfo $intent")

            if (session.player is MoosicBoxPlayer) {
                val player = session.player as MoosicBoxPlayer

                val key: KeyEvent? =
                        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                            intent.getParcelableExtra(Intent.EXTRA_KEY_EVENT, KeyEvent::class.java)
                        } else {
                            intent.getParcelableExtra(Intent.EXTRA_KEY_EVENT)
                        }

                if (key != null) {
                    when (key.keyCode) {
                        KeyEvent.KEYCODE_MEDIA_PLAY -> {
                            Log.i("PlaybackService", "onMediaButtonEvent play")
                        }
                        KeyEvent.KEYCODE_MEDIA_PAUSE -> {
                            Log.i("PlaybackService", "onMediaButtonEvent pause")
                        }
                        KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE -> {
                            Log.i("PlaybackService", "onMediaButtonEvent play/pause")
                            if (player.isPlaying) {
                                Log.i("PlaybackService", "onMediaButtonEvent play/pause pause")
                            } else {
                                Log.i("PlaybackService", "onMediaButtonEvent play/pause play")
                            }
                        }
                        KeyEvent.KEYCODE_MEDIA_NEXT -> {
                            Log.i("PlaybackService", "onMediaButtonEvent seekToNext")
                        }
                        KeyEvent.KEYCODE_MEDIA_PREVIOUS -> {
                            Log.i("PlaybackService", "onMediaButtonEvent seekToPrevious")
                        }
                        KeyEvent.KEYCODE_MEDIA_STOP -> {
                            Log.i("PlaybackService", "onMediaButtonEvent stop")
                        }
                    }
                }
            }

            return super.onMediaButtonEvent(session, controllerInfo, intent)
        }

        override fun onPlaybackResumption(
                session: MediaSession,
                controller: MediaSession.ControllerInfo
        ): ListenableFuture<MediaSession.MediaItemsWithStartPosition> {
            val settable = SettableFuture.create<MediaSession.MediaItemsWithStartPosition>()
            CoroutineScope(Dispatchers.Main).launch {
                val items = mutableListOf<MediaItem>()
                for (i in 0..(session.player.mediaItemCount - 1)) {
                    val item = session.player.getMediaItemAt(i)
                    items.add(item)
                }
                val resumptionPlaylist =
                        MediaSession.MediaItemsWithStartPosition(
                                items,
                                session.player.currentMediaItemIndex,
                                session.player.currentPosition
                        )
                settable.set(resumptionPlaylist)
            }
            return settable
        }
    }

    companion object {
        lateinit var instance: PlaybackService
    }
}
