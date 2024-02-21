import { InvokeArgs, invoke } from '@tauri-apps/api/core';
import { Api, Track, TrackType, trackId } from './services/api';
import { PlayerType, currentPlaybackSessionId } from './services/player';
import * as player from './services/player';
import { orderedEntries } from './services/util';

type PlaybackStatus = { playbackId: number };

enum PlayerAction {
    PLAY = 'player_play',
    PAUSE = 'player_pause',
    RESUME = 'player_resume',
    STOP_TRACK = 'player_stop_track',
    NEXT_TRACK = 'player_next_track',
    PREVIOUS_TRACK = 'player_previous_track',
    UPDATE_PLAYBACK = 'player_update_playback',
}

type TrackIdWithApiSource = {
    id: number;
    source: TrackType;
};

type UpdatePlayback = {
    sessionId: number;
    sessionPlaylistId: number;
    play?: boolean;
    stop?: boolean;
    playing?: boolean;
    quality?: Api.PlaybackQuality;
    position?: number;
    seek?: number;
    volume?: number;
    tracks?: TrackIdWithApiSource[];
};

async function invokePlayer(
    action: PlayerAction,
    args?: InvokeArgs,
): Promise<PlaybackStatus> {
    console.debug('invokePlayer', action, args);
    return (await invoke(action, args)) as PlaybackStatus;
}

function toTrackIdWithApiSource(track: Track) {
    return {
        id: trackId(track)!,
        source: track.type,
    };
}

async function updatePlayback(update: player.PlaybackUpdate): Promise<void> {
    console.debug('Received updatePlayback', update);

    const actions = {
        update: false,
    };

    const handler = {
        set<T = UpdatePlayback>(
            target: T,
            prop: keyof T,
            value: T[typeof prop],
        ): boolean {
            const existing = target[prop];

            if (existing !== value) {
                target[prop] = value;
                actions.update = true;
            }

            return true;
        },
    };

    const updatePlayback = new Proxy<UpdatePlayback>(
        {
            sessionId: update.sessionId,
            sessionPlaylistId:
                player.playerState.playbackSessions.find(
                    ({ sessionId }) => sessionId === update.sessionId,
                )?.playlist.sessionPlaylistId ?? -1,
        },
        handler,
    );

    for (const [key, value] of orderedEntries(update, [
        'stop',
        'play',
        'tracks',
        'position',
        'volume',
        'seek',
        'playing',
        'quality',
    ])) {
        if (typeof value === 'undefined') continue;

        switch (key) {
            case 'stop':
                updatePlayback.stop = value;
                break;
            case 'play':
                updatePlayback.play = value;
                break;
            case 'tracks':
                updatePlayback.tracks = value.map(toTrackIdWithApiSource);
                break;
            case 'position':
                updatePlayback.position = value;
                break;
            case 'volume':
                updatePlayback.volume = value;
                break;
            case 'seek':
                if (!updatePlayback.play && player.playing()) continue;
                updatePlayback.seek = value;
                break;
            case 'playing':
                updatePlayback.playing = value;
                break;
            case 'quality':
                updatePlayback.quality = value;
                break;
            case 'sessionId':
                break;
            default:
                key satisfies never;
        }
    }

    if (actions.update) {
        const playbackStatus = await invokePlayer(
            PlayerAction.UPDATE_PLAYBACK,
            updatePlayback,
        );

        console.debug('Updated playback:', playbackStatus);
    }
}

export function createPlayer(id: number): PlayerType {
    return {
        id,
        async activate() {
            const currentSesion = player.playerState.currentPlaybackSession;

            if (!currentSesion) {
                console.error('No current session');
            }

            const update: UpdatePlayback = {
                tracks: player.playlist().map(toTrackIdWithApiSource),
                position: player.playlistPosition(),
                seek: player.currentSeek(),
                volume: currentSesion?.volume,
                sessionId: currentPlaybackSessionId()!,
                sessionPlaylistId:
                    player.playerState.currentPlaybackSession?.playlist
                        .sessionPlaylistId ?? -1,
                quality: player.playbackQuality(),
            };
            await invokePlayer(PlayerAction.UPDATE_PLAYBACK, update);
        },
        updatePlayback,
    };
}
