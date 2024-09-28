import './playlist.css';
import { For, createEffect, createSignal, on } from 'solid-js';
import {
    playFromPlaylistPosition,
    playlist as playerPlaylist,
    playerState,
    playing,
    removeTrackFromPlaylist,
} from '~/services/player';
import Album from '../Album';
import { type Track, trackId } from '~/services/api';

export default function playlist() {
    const [playlist, setPlaylist] = createSignal<Track[]>([]);
    const [currentlyPlayingIndex, setCurrentlyPlayingIndex] =
        createSignal<number>();

    function updateCurrentlyPlayingIndex() {
        setCurrentlyPlayingIndex(
            playlist().findIndex(
                (track) => trackId(track) === trackId(playerState.currentTrack),
            ),
        );
    }

    createEffect(
        on(
            () => playerPlaylist(),
            (value) => {
                setPlaylist(value);
                updateCurrentlyPlayingIndex();
            },
        ),
    );

    createEffect(
        on(
            () => playerState.currentTrack,
            () => {
                updateCurrentlyPlayingIndex();
            },
        ),
    );

    return (
        <div class="playlist">
            <div class="playlist-tracks">
                <div class="playlist-tracks-play-queue">Play queue</div>
                <For each={playlist()}>
                    {(track, index) => (
                        <>
                            {trackId(playerState.currentTrack) ===
                                trackId(track) && (
                                <div class="playlist-tracks-playing-from">
                                    Playing from:{' '}
                                    <a href={`/albums/${track.albumId}`}>
                                        {track.album}
                                    </a>
                                </div>
                            )}
                            {index() === (currentlyPlayingIndex() ?? 0) + 1 && (
                                <div class="playlist-tracks-next-up">
                                    Next up:
                                </div>
                            )}
                            <div
                                class={`playlist-tracks-track${
                                    trackId(playerState.currentTrack) ===
                                    trackId(track)
                                        ? ' current'
                                        : ''
                                }${
                                    trackId(playerState.currentTrack) ===
                                        trackId(track) && playing()
                                        ? ' playing'
                                        : ''
                                }${
                                    index() < (currentlyPlayingIndex() ?? 0)
                                        ? ' past'
                                        : ''
                                }`}
                                onClick={() =>
                                    index() !== currentlyPlayingIndex() &&
                                    playFromPlaylistPosition(index())
                                }
                            >
                                <div class="playlist-tracks-track-album-artwork">
                                    <div class="playlist-tracks-track-album-artwork-icon">
                                        <Album
                                            album={track}
                                            size={50}
                                            route={false}
                                        />
                                        {index() === currentlyPlayingIndex() ? (
                                            <img
                                                class="audio-icon"
                                                src="/img/audio-white.svg"
                                                alt="Playing"
                                            />
                                        ) : (
                                            <img
                                                class="play-icon"
                                                src="/img/play-button-white.svg"
                                                alt="Playing"
                                            />
                                        )}
                                    </div>
                                </div>
                                <div class="playlist-tracks-track-details">
                                    <div class="playlist-tracks-track-details-title">
                                        {track.title}
                                    </div>
                                    <div class="playlist-tracks-track-details-artist">
                                        {track.artist}
                                    </div>
                                </div>
                                {index() !== (currentlyPlayingIndex() ?? 0) && (
                                    <div
                                        class="playlist-tracks-track-remove"
                                        onClick={(e) => {
                                            removeTrackFromPlaylist(index());
                                            e.stopImmediatePropagation();
                                        }}
                                    >
                                        <img
                                            class="cross-icon"
                                            src="/img/cross-white.svg"
                                            alt="Remove from queue"
                                        />
                                    </div>
                                )}
                            </div>
                        </>
                    )}
                </For>
            </div>
        </div>
    );
}
