package com.moosicbox.playerplugin

import android.app.Activity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Channel
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

data class Track(
        val id: String = "",
        val number: Int = 0,
        val title: String = "",
        val album: String = "",
        val albumCover: String? = null,
        val artist: String = "",
        val artistCover: String? = null,
        val duration: Double = 0.0,
)

data class Playlist(val tracks: List<Track> = listOf())

@InvokeArg
data class State(
        val playing: Boolean? = null,
        val position: Long? = null,
        val seek: Double? = null,
        val volume: Double? = null,
        val playlist: Playlist? = null,
)

@InvokeArg data class InitChannel(val channel: Channel? = null)

data class MediaEvent(
        val play: Boolean? = null,
        val nextTrack: Boolean? = null,
        val prevTrack: Boolean? = null,
)

@TauriPlugin
class PlayerPlugin(private val activity: Activity) : Plugin(activity) {
    private val implementation = Player()

    @Command
    fun initChannel(invoke: Invoke) {
        val args = invoke.parseArgs(InitChannel::class.java)

        val ret = JSObject()
        Player.channel = args.channel!!
        invoke.resolve(ret)
    }

    @Command
    fun updateState(invoke: Invoke) {
        val args = invoke.parseArgs(State::class.java)

        val ret = JSObject()
        implementation.updateState(args)
        invoke.resolve(ret)
    }
}
