import './playback-sessions.css';
import { For, Index, createComputed, createSignal } from 'solid-js';
import { Api } from '~/services/api';
import type { Track } from '~/services/api';
import { playerState, setPlayerState, updateSession } from '~/services/player';
import Album from '../Album';
import { produce } from 'solid-js/store';
import { wsService } from '~/services/ws';

const queuedTracksCache: {
    [id: number]: { position?: number; tracks: Track[] };
} = {};

export default function playbackSessionsFunc() {
    const [sessions, setSessions] = createSignal<Api.PlaybackSession[]>(
        playerState.playbackSessions,
    );
    const [audioZoneSession, setAudioZoneSession] =
        createSignal<Api.PlaybackSession>();

    createComputed(() => {
        setSessions(playerState.playbackSessions);

        if (audioZoneSession()) {
            setAudioZoneSession(
                sessions().find(
                    (s) => s.sessionId === audioZoneSession()?.sessionId,
                ),
            );
        }
    });

    function deleteSession(sessionId: number) {
        if (sessionId === playerState.currentPlaybackSession?.sessionId) {
            setPlayerState(
                produce((state) => {
                    state.playbackSessions.find(
                        (s) => s.sessionId === sessionId,
                    );
                    setSessions(
                        sessions().filter((s) => s.sessionId !== sessionId),
                    );
                    const newSession = sessions()[0];
                    if (newSession) {
                        updateSession(state, newSession, true);
                    }
                }),
            );
        }
        wsService.deleteSession(sessionId);
    }

    function activateSession(session: Api.PlaybackSession) {
        if (session.sessionId === playerState.currentPlaybackSession?.sessionId)
            return;
        setPlayerState(
            produce((state) => {
                updateSession(state, session, true);
            }),
        );
    }

    function queuedTracks(session: Api.PlaybackSession) {
        const cache = queuedTracksCache[session.sessionId];

        if (
            cache?.position === session.position &&
            cache?.tracks.every((t, i) => {
                const track = session.playlist.tracks[i];

                if (!track) {
                    console.error('Failed to queue tracks');
                    return undefined;
                }

                return (
                    ('trackId' in track &&
                        'trackId' in t &&
                        track.trackId === t?.trackId) ||
                    ('id' in track && 'id' in t && track.id === t?.id)
                );
            })
        ) {
            return cache.tracks;
        }

        const tracks = session.playlist.tracks.slice(
            session.position ?? 0,
            session.playlist.tracks.length,
        );
        queuedTracksCache[session.sessionId] = {
            position: session.position!,
            tracks,
        };

        return tracks;
    }

    return (
        <div class="playback-sessions">
            <div class="playback-sessions-list">
                <For each={playerState.playbackSessions}>
                    {(session) => (
                        <div
                            class={`playback-sessions-list-session${
                                playerState.currentPlaybackSession
                                    ?.sessionId === session.sessionId
                                    ? ' active'
                                    : ''
                            }`}
                        >
                            <div
                                class="playback-sessions-list-session-header"
                                onClick={() => activateSession(session)}
                            >
                                <img
                                    class="playback-sessions-list-session-header-speaker-icon"
                                    src="/img/speaker-white.svg"
                                />
                                <h2 class="playback-sessions-list-session-header-session-name">
                                    {session.name}
                                </h2>
                                <h3 class="playback-sessions-list-session-header-session-tracks-queued">
                                    {queuedTracks(session).length} track
                                    {queuedTracks(session).length === 1
                                        ? ''
                                        : 's'}{' '}
                                    queued
                                </h3>
                                {playerState.currentPlaybackSession
                                    ?.sessionId === session.sessionId && (
                                    <>
                                        <img
                                            class="playback-sessions-list-session-header-checkmark-icon"
                                            src="/img/checkmark-white.svg"
                                        />
                                    </>
                                )}
                                {session.playing && (
                                    <img
                                        class="playback-sessions-list-session-header-playing-icon"
                                        src="/img/audio-white.svg"
                                    />
                                )}
                                <div class="playback-sessions-list-session-header-right">
                                    <div
                                        class="playback-sessions-list-session-header-delete-session"
                                        onClick={(e) => {
                                            deleteSession(session.sessionId);
                                            e.stopImmediatePropagation();
                                        }}
                                    >
                                        <img
                                            class="trash-icon"
                                            src="/img/trash-white.svg"
                                            alt="Delete playback session"
                                        />
                                    </div>
                                </div>
                            </div>
                            <div class="playback-sessions-playlist-tracks-container">
                                <div class="playback-sessions-playlist-tracks">
                                    <Index each={queuedTracks(session)}>
                                        {(track, index) =>
                                            index >= 4 ? (
                                                <></>
                                            ) : (
                                                <div class="playback-sessions-playlist-tracks-track">
                                                    <div class="playback-sessions-playlist-tracks-track-album-artwork">
                                                        <div class="playback-sessions-playlist-tracks-track-album-artwork-icon">
                                                            <Album
                                                                album={track()}
                                                                size={40}
                                                                route={false}
                                                            />
                                                        </div>
                                                    </div>
                                                    <div class="playback-sessions-playlist-tracks-track-details">
                                                        <div class="playback-sessions-playlist-tracks-track-details-title">
                                                            {track().title}
                                                        </div>
                                                        <div class="playback-sessions-playlist-tracks-track-details-artist">
                                                            {track().artist}
                                                        </div>
                                                    </div>
                                                </div>
                                            )
                                        }
                                    </Index>
                                </div>
                                {queuedTracks(session).length >= 3 && (
                                    <div class="playback-sessions-playlist-tracks-overlay"></div>
                                )}
                            </div>
                        </div>
                    )}
                </For>
            </div>
        </div>
    );
}
