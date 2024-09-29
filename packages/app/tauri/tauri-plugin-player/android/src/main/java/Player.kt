package com.moosicbox.playerplugin

import android.util.Log
import app.tauri.plugin.Channel
import app.tauri.plugin.JSObject

class Player {
    fun updateState(state: State) {
        Player.updateState(state)
    }

    companion object {
        public lateinit var channel: Channel
        public lateinit var updateState: (State) -> Unit

        fun sendMediaEvent(event: MediaEvent) {
            val obj = JSObject()

            if (event.play != null) {
                obj.put("play", event.play)
            }
            if (event.nextTrack != null) {
                obj.put("nextTrack", event.nextTrack)
            }
            if (event.prevTrack != null) {
                obj.put("prevTrack", event.prevTrack)
            }

            if (!::channel.isInitialized) {
                Log.e("Player", "Channel is not initialized")
                return
            }

            channel.send(obj)
        }
    }
}
