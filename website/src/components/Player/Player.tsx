import {
    Show,
    createComputed,
    createEffect,
    createSignal,
    on,
    onCleanup,
    onMount,
} from 'solid-js';
import './Player.css';
import {
    currentSeek,
    currentTrackLength,
    nextTrack,
    offNextTrack,
    offPreviousTrack,
    onNextTrack,
    onPreviousTrack,
    pause,
    play,
    playing as playerPlaying,
    playerState,
    previousTrack,
} from '~/services/player';
import { toTime } from '~/services/formatting';
import { isServer } from 'solid-js/web';
import Album from '../Album';
import Playlist from '../Playlist';
import {
    showAudioZones,
    showPlaybackQuality,
    showPlaybackSessions,
} from '~/services/app';
import Volume from '../Volume';
import { albumRoute } from '../Album/Album';
import { artistRoute } from '../Artist/Artist';
import { clientSignal } from '~/services/util';
import Visualization from '../Visualization';

function getTrackDuration() {
    return playerState.currentTrack?.duration ?? currentTrackLength();
}

let playlistSlideoutTimeout: NodeJS.Timeout | undefined;

enum BackToNowPlayingPosition {
    top = 'TOP',
    bottom = 'BOTTOM',
    none = 'NONE',
}

export default function player() {
    let playlistSlideout: HTMLDivElement | undefined;
    let playlistSlideoutContentRef: HTMLDivElement | undefined;
    let backToNowPlayingTopRef: HTMLDivElement | undefined;
    let backToNowPlayingBottomRef: HTMLDivElement | undefined;
    let playerRef: HTMLDivElement | undefined;
    const [showingPlaylist, setShowingPlaylist] = createSignal(false);
    const [playing, setPlaying] = createSignal(playerPlaying());
    const [showTrackOptionsMobile, setShowTrackOptionsMobile] =
        createSignal(false);

    const [$showAudioZones] = clientSignal(showAudioZones);
    const [$showPlaybackSessions] = clientSignal(showPlaybackSessions);
    const [$showPlaybackQuality] = clientSignal(showPlaybackQuality);

    createComputed(() => {
        setPlaying(playerState.currentPlaybackSession?.playing ?? false);
    });

    function closePlaylist() {
        if (!showingPlaylist()) return;

        setShowingPlaylist(false);
        playlistSlideoutTimeout = setTimeout(() => {
            playlistSlideout!.style.display = 'none';
            playlistSlideoutTimeout = undefined;
        }, 200);
    }

    function openPlaylist() {
        if (showingPlaylist()) return;

        if (playlistSlideoutTimeout) {
            clearTimeout(playlistSlideoutTimeout);
        }
        playlistSlideout!.style.display = 'block';
        scrollPlaylistToNowPlaying(true);
        setTimeout(() => {
            setShowingPlaylist(true);
        }, 0);
    }

    function togglePlaylist() {
        if (showingPlaylist()) {
            closePlaylist();
        } else {
            openPlaylist();
        }
    }

    function toggleShowTrackOptionsMobile() {
        setShowTrackOptionsMobile(!showTrackOptionsMobile());
    }

    function toggleShowPlaybackQuality() {
        showPlaybackQuality.set(!$showPlaybackQuality());
    }

    function toggleShowAudioZones() {
        showAudioZones.set(!$showAudioZones());
    }

    function toggleShowPlaybackSessions() {
        showPlaybackSessions.set(!$showPlaybackSessions());
    }

    createEffect(
        on(
            () => location.pathname,
            () => {
                closePlaylist();
            },
        ),
    );

    const handleClick = (event: MouseEvent) => {
        if (
            !playlistSlideout?.contains(event.target as Node) &&
            !playerRef?.contains(event.target as Node)
        ) {
            closePlaylist();
        }
    };

    onMount(() => {
        if (isServer) return;
        window.addEventListener('click', handleClick);
    });

    onCleanup(() => {
        if (isServer) return;
        window.removeEventListener('click', handleClick);
    });

    let nextTrackListener: () => void;
    let previousTrackListener: () => void;

    onMount(() => {
        onNextTrack(
            (nextTrackListener = () => {
                if (!showingPlaylist()) return;
                scrollPlaylistToNowPlaying();
            }),
        );
        onPreviousTrack(
            (previousTrackListener = () => {
                if (!showingPlaylist()) return;
                scrollPlaylistToNowPlaying();
            }),
        );
    });

    onCleanup(() => {
        offNextTrack(nextTrackListener);
        offPreviousTrack(previousTrackListener);
    });

    const [backToNowPlayingPosition, setBackToNowPlayingPosition] =
        createSignal(BackToNowPlayingPosition.none);

    let backToNowPlayingTopTimeout: NodeJS.Timeout;
    let backToNowPlayingBottomTimeout: NodeJS.Timeout;
    const scrollListener = () => {
        if (!getCurrentTrack()) return;

        if (
            getCurrentTrack()!.getBoundingClientRect().top >
            playlistSlideout!.offsetHeight
        ) {
            clearTimeout(backToNowPlayingBottomTimeout);
            setBackToNowPlayingPosition(BackToNowPlayingPosition.bottom);
            backToNowPlayingTopRef!.style.opacity = '0';
            backToNowPlayingBottomRef!.style.display = 'block';
            setTimeout(() => {
                backToNowPlayingBottomRef!.style.opacity = '1';
            }, 0);
        } else if (getCurrentTrack()!.getBoundingClientRect().bottom < 0) {
            clearTimeout(backToNowPlayingTopTimeout);
            setBackToNowPlayingPosition(BackToNowPlayingPosition.top);
            backToNowPlayingBottomRef!.style.opacity = '0';
            backToNowPlayingTopRef!.style.display = 'block';
            setTimeout(() => {
                backToNowPlayingTopRef!.style.opacity = '1';
            }, 0);
        } else {
            backToNowPlayingTopRef!.style.opacity = '0';
            backToNowPlayingBottomRef!.style.opacity = '0';
            if (backToNowPlayingPosition() === BackToNowPlayingPosition.top) {
                backToNowPlayingTopTimeout = setTimeout(() => {
                    backToNowPlayingTopRef!.style.display = 'none';
                }, 300);
            } else if (
                backToNowPlayingPosition() === BackToNowPlayingPosition.bottom
            ) {
                backToNowPlayingBottomTimeout = setTimeout(() => {
                    backToNowPlayingBottomRef!.style.display = 'none';
                }, 300);
            }
            setBackToNowPlayingPosition(BackToNowPlayingPosition.none);
        }
    };

    onMount(() => {
        if (isServer) return;
        playlistSlideoutContentRef?.addEventListener('scroll', scrollListener);

        scrollListener();
    });

    onCleanup(() => {
        if (isServer) return;
        playlistSlideoutContentRef?.removeEventListener(
            'scroll',
            scrollListener,
        );
    });

    function getPlayingFrom(): Element | null {
        return (
            playlistSlideout?.querySelector('.playlist-tracks-playing-from') ??
            null
        );
    }

    function getCurrentTrack(): Element | null {
        return (
            playlistSlideout?.querySelector('.playlist-tracks-track.current') ??
            null
        );
    }

    function scrollPlaylistToNowPlaying(instant = false) {
        getPlayingFrom()?.scrollIntoView({
            behavior: instant ? 'instant' : 'smooth',
        });
    }

    return (
        <>
            <div ref={playerRef!} class="player">
                <Visualization></Visualization>
                <div class="player-controls">
                    <div class="player-now-playing">
                        <div class="player-album-details">
                            <Show when={playerState.currentTrack}>
                                {(currentTrack) => (
                                    <>
                                        <div class="player-album-details-icon">
                                            <Album
                                                album={currentTrack()}
                                                size={70}
                                                artist={false}
                                                title={false}
                                            />
                                        </div>
                                        <div class="player-now-playing-details">
                                            <div class="player-now-playing-details-title">
                                                <a
                                                    href={albumRoute(
                                                        currentTrack(),
                                                    )}
                                                    title={currentTrack().title}
                                                >
                                                    {currentTrack().title}
                                                </a>
                                            </div>
                                            <div class="player-now-playing-details-artist">
                                                <a
                                                    href={artistRoute(
                                                        currentTrack(),
                                                    )}
                                                    title={
                                                        currentTrack().artist
                                                    }
                                                >
                                                    {currentTrack().artist}
                                                </a>
                                            </div>
                                            <div class="player-now-playing-details-album">
                                                Playing from:{' '}
                                                <a
                                                    href={albumRoute(
                                                        currentTrack(),
                                                    )}
                                                    title={currentTrack().album}
                                                >
                                                    {currentTrack().album}
                                                </a>
                                            </div>
                                        </div>
                                    </>
                                )}
                            </Show>
                        </div>
                    </div>
                    <div class="player-media-controls">
                        <div class="player-media-controls-track">
                            <button
                                class="media-button button"
                                onClick={() => previousTrack()}
                            >
                                <img
                                    class="previous-track-button"
                                    src="/img/next-button-white.svg"
                                    alt="Previous Track"
                                />
                            </button>
                            <button
                                class="media-button button"
                                onClick={() => pause()}
                                style={{
                                    display: playing() ? 'initial' : 'none',
                                }}
                            >
                                <img
                                    class="pause-button"
                                    src="/img/pause-button-white.svg"
                                    alt="Pause"
                                />
                            </button>
                            <button
                                class="media-button button"
                                onClick={() => play()}
                                style={{
                                    display: !playing() ? 'initial' : 'none',
                                }}
                            >
                                <img
                                    class="play-button"
                                    src="/img/play-button-white.svg"
                                    alt="Play"
                                />
                            </button>
                            <button
                                class="media-button button"
                                onClick={() => nextTrack()}
                            >
                                <img
                                    class="next-track-button"
                                    src="/img/next-button-white.svg"
                                    alt="Next Track"
                                />
                            </button>
                            <img
                                class="show-playback-quality-icon"
                                src="/img/more-options-white.svg"
                                alt="Show Playback Quality"
                                onClick={() => toggleShowPlaybackQuality()}
                            />
                        </div>
                        <div class="player-media-controls-seeker">
                            <span class="player-media-controls-seeker-current-time">
                                {toTime(currentSeek() ?? 0)}
                            </span>
                            //
                            <span class="player-media-controls-seeker-total-time">
                                {toTime(getTrackDuration())}
                            </span>
                        </div>
                    </div>
                    <div class="player-track-options">
                        <div class="player-track-options-buttons">
                            <Volume />
                            <img
                                class="show-audio-zones-icon"
                                src="/img/speaker-white.svg"
                                alt="Configure Audio Outputs"
                                onClick={() => toggleShowAudioZones()}
                            />
                            <img
                                class="show-playback-sessions-icon"
                                src="/img/sessions-white.svg"
                                alt="Show Playback Sessions"
                                onClick={() => toggleShowPlaybackSessions()}
                            />
                            <img
                                class="show-playlist-icon"
                                src="/img/playlist-white.svg"
                                alt="Show Playlist"
                                onClick={() => togglePlaylist()}
                            />
                        </div>
                        <div class="player-track-options-mobile">
                            <img
                                class="mobile-playback-options"
                                src="/img/more-options-white.svg"
                                alt="Show Playback Options"
                                onClick={() => toggleShowTrackOptionsMobile()}
                            />
                            <img
                                class="show-playlist-icon"
                                src="/img/playlist-white.svg"
                                alt="Show Playlist"
                                onClick={() => togglePlaylist()}
                            />
                        </div>
                    </div>
                </div>
                <div
                    class={`player-track-options-mobile-buttons${
                        showTrackOptionsMobile() ? ' visible' : ' hidden'
                    }`}
                >
                    <Volume />
                    <img
                        class="show-audio-zones-icon"
                        src="/img/speaker-white.svg"
                        alt="Configure Audio Outputs"
                        onClick={() => toggleShowAudioZones()}
                    />
                    <img
                        class="show-playback-sessions-icon"
                        src="/img/sessions-white.svg"
                        alt="Show Playback Sessions"
                        onClick={() => toggleShowPlaybackSessions()}
                    />
                    <img
                        class="show-playback-quality-icon"
                        src="/img/more-options-white.svg"
                        alt="Show Playback Quality"
                        onClick={() => toggleShowPlaybackQuality()}
                    />
                </div>
                <div
                    class="playlist-slideout"
                    ref={playlistSlideout!}
                    style={{
                        transform: `translateX(${showingPlaylist() ? 0 : 100}%)`,
                    }}
                >
                    <div
                        ref={playlistSlideoutContentRef!}
                        class="playlist-slideout-content"
                    >
                        <Playlist />
                    </div>
                    <div
                        ref={backToNowPlayingTopRef!}
                        class="playlist-slideout-back-to-now-playing-top"
                        onClick={() => scrollPlaylistToNowPlaying()}
                    >
                        Back to now playing
                    </div>
                    <div
                        ref={backToNowPlayingBottomRef!}
                        class="playlist-slideout-back-to-now-playing-bottom"
                        onClick={() => scrollPlaylistToNowPlaying()}
                    >
                        Back to now playing
                    </div>
                </div>
            </div>
        </>
    );
}
