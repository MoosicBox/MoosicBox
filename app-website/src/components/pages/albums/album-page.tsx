import './album-page.css';
import {
    createComputed,
    createEffect,
    createSignal,
    For,
    on,
    onCleanup,
    onMount,
    Show,
} from 'solid-js';
import { isServer } from 'solid-js/web';
import Album from '~/components/Album';
import {
    displayAlbumVersionQuality,
    displayDate,
    toTime,
} from '~/services/formatting';
import { addTracksToQueue, playerState, playPlaylist } from '~/services/player';
import { Api, api, ApiSource, trackId } from '~/services/api';
import { artistRoute } from '~/components/Artist/Artist';
import { areEqualShallow, historyBack } from '~/services/util';

export default function albumPage(props: {
    albumId?: number;
    tidalAlbumId?: string;
    qobuzAlbumId?: string;
}) {
    const [versions, setVersions] = createSignal<Api.AlbumVersion[]>();
    const [showingArtwork, setShowingArtwork] = createSignal(false);
    const [blurringArtwork, setBlurringArtwork] = createSignal<boolean>();
    const [sourceImage, setSourceImage] = createSignal<HTMLImageElement>();
    const [activeVersion, setActiveVersion] = createSignal<Api.AlbumVersion>();

    const [album, setAlbum] = createSignal<Api.Album>();

    const [showTrackOptions, setShowTrackOptions] = createSignal<
        string | number
    >();

    const handleShowTrackOptionsClick = (_event: MouseEvent) => {
        if (!showTrackOptions()) return;
        setShowTrackOptions(undefined);
    };

    onMount(() => {
        if (isServer) return;
        window.addEventListener('click', handleShowTrackOptionsClick);
    });

    onCleanup(() => {
        if (isServer) return;
        window.removeEventListener('click', handleShowTrackOptionsClick);
    });

    let sourceImageRef: HTMLImageElement | undefined;

    function getTracks(): Api.Track[] | undefined {
        return activeVersion()?.tracks;
    }

    async function loadAlbum() {
        try {
            if (props.albumId) {
                setAlbum(await api.getAlbum(props.albumId));
            } else if (props.tidalAlbumId) {
                setAlbum(await api.getAlbum(props.tidalAlbumId, 'TIDAL'));
            } else if (props.qobuzAlbumId) {
                setAlbum(await api.getAlbum(props.qobuzAlbumId, 'QOBUZ'));
            }
        } catch {
            setAlbum(undefined);
        }
    }

    async function loadVersions() {
        if (props.albumId) {
            setVersions(await api.getAlbumVersions(props.albumId));
        } else if (props.tidalAlbumId) {
            setVersions(
                await api.getAlbumVersions(props.tidalAlbumId, 'TIDAL'),
            );
        } else if (props.qobuzAlbumId) {
            setVersions(
                await api.getAlbumVersions(props.qobuzAlbumId, 'QOBUZ'),
            );
        } else {
            throw new Error('Invalid album type');
        }

        if (activeVersion()) {
            const version = versions()?.find((v) =>
                areEqualShallow(v, activeVersion()!),
            );
            setActiveVersion(version ?? versions()?.[0]);
        } else {
            setActiveVersion(versions()?.[0]);
        }

        return versions;
    }

    function addEmptyVersion(source: Api.TrackSource) {
        setVersions([
            ...versions()!,
            {
                tracks: [],
                format: null,
                bitDepth: null,
                audioBitrate: null,
                overallBitrate: null,
                sampleRate: null,
                channels: null,
                source,
            },
        ]);
    }

    async function loadDetails() {
        const prevActive = activeVersion();

        await Promise.all([loadAlbum(), loadVersions()]);

        if (props.tidalAlbumId && isInvalidFavorite()) {
            addEmptyVersion(Api.TrackSource.TIDAL);
        }
        if (props.qobuzAlbumId && isInvalidFavorite()) {
            addEmptyVersion(Api.TrackSource.QOBUZ);
        }

        if (versions()) {
            if (prevActive) {
                setActiveVersion(
                    versions()!.find(
                        (version) => version.source === prevActive.source,
                    ),
                );
            }
            if (!activeVersion()) {
                setActiveVersion(versions()![0]);
            }
        }
    }

    async function addAlbumToLibrary() {
        const source = props.tidalAlbumId
            ? Api.TrackSource.TIDAL
            : props.qobuzAlbumId
              ? Api.TrackSource.QOBUZ
              : undefined;

        if (!source) {
            throw new Error(
                `Invalid add album request: ${JSON.stringify(props)}`,
            );
        }

        switch (source) {
            case Api.TrackSource.TIDAL: {
                await api.addAlbumToLibrary({
                    tidalAlbumId: props.tidalAlbumId!,
                });
                await loadDetails();
                if (versions()) {
                    setActiveVersion(
                        versions()!.find(
                            (version) =>
                                version.source === Api.TrackSource.TIDAL,
                        ),
                    );
                }
                break;
            }
            case Api.TrackSource.QOBUZ: {
                await api.addAlbumToLibrary({
                    qobuzAlbumId: props.qobuzAlbumId!,
                });
                await loadDetails();
                if (versions()) {
                    setActiveVersion(
                        versions()!.find(
                            (version) =>
                                version.source === Api.TrackSource.QOBUZ,
                        ),
                    );
                }
                break;
            }
            default:
                source satisfies never;
        }
    }

    async function refavoriteAlbum(albumId: {
        tidalAlbumId?: string | number;
        qobuzAlbumId?: string | number;
    }) {
        const refavoritedAlbum = await api.refavoriteAlbum(albumId);

        if (!shouldNavigate) {
            return;
        }

        if (refavoritedAlbum.albumId !== album()?.albumId) {
            //navigate(albumRoute(album), { replace: true });
        } else {
            await loadDetails();
        }
    }

    async function downloadAlbum(source: Api.DownloadApiSource) {
        console.debug('Downloading album from source:', source);
        switch (source) {
            case 'QOBUZ':
                await api.download(
                    {
                        albumId: album()?.albumSources.find(
                            (x) => x.source === 'QOBUZ',
                        )?.id,
                    },
                    source,
                );
                break;
            case 'TIDAL':
                await api.download(
                    {
                        albumId: album()?.albumSources.find(
                            (x) => x.source === 'TIDAL',
                        )?.id,
                    },
                    source,
                );
                break;
            case 'YT':
                await api.download(
                    {
                        albumId: album()?.albumSources.find(
                            (x) => x.source === 'YT',
                        )?.id,
                    },
                    source,
                );
                break;
        }
    }

    let shouldNavigate = true;

    async function removeAlbumFromLibrary(albumId: {
        tidalAlbumId?: string | number;
        qobuzAlbumId?: string | number;
    }) {
        const source = albumId.tidalAlbumId
            ? Api.TrackSource.TIDAL
            : albumId.qobuzAlbumId
              ? Api.TrackSource.QOBUZ
              : undefined;

        if (!source) {
            throw new Error(
                `Invalid remove album request: ${JSON.stringify(albumId)}`,
            );
        }

        const album = await api.removeAlbumFromLibrary(albumId);

        if (!shouldNavigate) {
            return;
        }

        const removedEveryVersion =
            !versions() ||
            versions()!.every((version) => version.source === source);

        if (removedEveryVersion) {
            setAlbum(undefined);
        } else {
            if (props.albumId) {
                setAlbum(album);
            } else {
                setAlbum(undefined);
            }

            if (versions()) {
                setVersions(
                    versions()!.filter((version) => version.source !== source),
                );
                if (activeVersion()?.source === source) {
                    setActiveVersion(versions()![0]);
                }
            }
        }
    }

    createEffect(
        on(
            () => props.albumId,
            (value, prev) => {
                if (value !== prev) loadPage();
            },
        ),
    );

    createEffect(
        on(
            () => props.tidalAlbumId,
            (value, prev) => {
                if (value !== prev) loadPage();
            },
        ),
    );

    createEffect(
        on(
            () => props.qobuzAlbumId,
            (value, prev) => {
                if (value !== prev) loadPage();
            },
        ),
    );

    let loaded = false;

    async function loadPage() {
        if (loaded) {
            shouldNavigate = false;
        }
        loaded = true;

        setAlbum(undefined);
        setVersions(undefined);
        setShowingArtwork(false);
        setBlurringArtwork(undefined);
        setSourceImage(undefined);
        setActiveVersion(undefined);

        if (isServer) return;

        await loadDetails();
    }

    function isInvalidFavorite() {
        if (!versions() || !album()) {
            return false;
        }

        let targetSource: ApiSource = 'LIBRARY';

        if (props.tidalAlbumId) {
            targetSource = 'TIDAL';
        }
        if (props.qobuzAlbumId) {
            targetSource = 'QOBUZ';
        }

        if (!album()?.albumSources.find((x) => x.source === targetSource)?.id) {
            return false;
        }

        const version = versions()!.find(
            (version) => version.source === targetSource,
        );

        return !version || version.tracks.length === 0;
    }

    async function playAlbumFrom(track: Api.Track) {
        const tracks = getTracks()!;
        const playlist = tracks.slice(tracks.indexOf(track));

        playPlaylist(playlist);
    }

    function albumDuration(): number {
        let duration = 0;

        const tracks = getTracks()!;
        tracks.forEach((track) => (duration += track.duration));

        return duration;
    }

    createComputed(() => {
        setBlurringArtwork(album()?.blur);
    });

    createEffect(
        on(
            () => showingArtwork(),
            (showing) => {
                if (!sourceImage() && showing && sourceImageRef) {
                    sourceImageRef.src = api.getAlbumSourceArtwork(album());
                    sourceImageRef.onload = ({ target }) => {
                        const image = target as HTMLImageElement;
                        setSourceImage(image);
                    };
                }
            },
        ),
    );

    function toggleBlurringArtwork() {
        setBlurringArtwork(!blurringArtwork());
    }

    function showArtwork(): void {
        setBlurringArtwork(album()?.blur);
        setSourceImage(undefined);
        setShowingArtwork(true);
        setTimeout(() => {
            window.addEventListener('click', handleClick);
        });
    }

    function hideArtwork(): void {
        setShowingArtwork(false);
        setTimeout(() => {
            window.removeEventListener('click', handleClick);
        });
    }

    let albumArtworkPreviewerIcon: HTMLImageElement | undefined;

    const handleClick = (event: MouseEvent) => {
        const node = event.target as Node;
        if (
            !albumArtworkPreviewerIcon?.contains(node) &&
            !sourceImageRef?.contains(node)
        ) {
            hideArtwork();
        }
    };

    onCleanup(() => {
        shouldNavigate = false;

        if (isServer) return;

        window.removeEventListener('click', handleClick);
    });

    function albumArtworkPreviewer() {
        return (
            <div class="album-page-artwork-previewer">
                <div class="album-page-artwork-previewer-content">
                    <img
                        ref={sourceImageRef!}
                        alt={`${album()?.title} by ${album()?.artist}`}
                        style={{
                            cursor: album()?.blur ? 'pointer' : 'initial',
                            visibility: blurringArtwork()
                                ? 'hidden'
                                : undefined,
                        }}
                        onClick={() => album()?.blur && toggleBlurringArtwork()}
                    />
                    <Show when={blurringArtwork() && sourceImage()}>
                        <img
                            ref={albumArtworkPreviewerIcon!}
                            src={api.getAlbumArtwork(album(), 16, 16)}
                            style={{
                                'image-rendering': 'pixelated',
                                cursor: 'pointer',
                                width: '100%',
                                position: 'absolute',
                                left: '0',
                                top: '0',
                            }}
                            onClick={() =>
                                album()?.blur && toggleBlurringArtwork()
                            }
                        />
                    </Show>
                    {blurringArtwork() && (
                        <div class="album-page-artwork-previewer-content-blur-notice">
                            Click to unblur
                        </div>
                    )}
                </div>
            </div>
        );
    }

    function getTrackTitleDisplay(track: Api.Track): string {
        return track.title;
    }

    function isExplicit(_track: Api.Track): boolean {
        return false;
    }

    function track(track: Api.Track) {
        return (
            <tr
                class={`album-page-tracks-track${
                    trackId(playerState.currentTrack) === trackId(track)
                        ? ' playing'
                        : ''
                }`}
                onDblClick={() => playAlbumFrom(track)}
            >
                <td
                    class="album-page-tracks-track-no"
                    onClick={() => playAlbumFrom(track)}
                >
                    <div class="album-page-tracks-track-no-container">
                        {trackId(playerState.currentTrack) ===
                        trackId(track) ? (
                            <img
                                class="audio-icon"
                                src="/img/audio-white.svg"
                                alt="Playing"
                            />
                        ) : (
                            <span class="track-no-text">{track.number}</span>
                        )}
                        <img
                            class="play-button"
                            src="/img/play-button-white.svg"
                            alt="Play"
                        />
                    </div>
                </td>
                <td class="album-page-tracks-track-title">
                    {getTrackTitleDisplay(track)}
                    {isExplicit(track) && (
                        <img
                            class="album-page-tracks-track-title-explicit"
                            src="/img/explicit.svg"
                            alt="Explicit"
                        />
                    )}
                </td>
                <td class="album-page-tracks-track-artist">
                    <a
                        href={artistRoute(track)}
                        class="album-page-tracks-track-artist-text"
                    >
                        {track.artist}
                    </a>
                </td>
                <td class="album-page-tracks-track-time">
                    <div class="album-page-tracks-track-time-content">
                        <div class="album-page-tracks-track-time-content-duration">
                            {toTime(Math.round(track.duration))}
                        </div>
                        <div class="album-page-tracks-track-time-content-options">
                            <button
                                style="position: relative"
                                class="remove-button-styles"
                                onClick={(e) => {
                                    if (showTrackOptions() === track.trackId) {
                                        setShowTrackOptions(undefined);
                                    } else {
                                        setShowTrackOptions(track.trackId);
                                    }
                                    e.stopPropagation();
                                    e.preventDefault();
                                }}
                                onDblClick={(e) => {
                                    e.stopPropagation();
                                    e.preventDefault();
                                }}
                            >
                                <img
                                    class="more-options more-options-button"
                                    src="/img/more-options-white.svg"
                                    alt="Options"
                                />
                                {showTrackOptions() == track.trackId && (
                                    <div class="moosicbox-select">
                                        <div class="moosicbox-select-option">
                                            <div
                                                onClick={async () => {
                                                    await addTracksToQueue([
                                                        track,
                                                    ]);
                                                }}
                                            >
                                                Add to queue
                                            </div>
                                        </div>
                                    </div>
                                )}
                            </button>
                        </div>
                    </div>
                </td>
                <td></td>
            </tr>
        );
    }

    return (
        <div>
            <div class="album-page-container">
                <div class="album-page">
                    <div class="album-page-breadcrumbs">
                        <a
                            class="back-button"
                            href="#"
                            onClick={() => historyBack()}
                        >
                            Back
                        </a>
                    </div>
                    <div class="album-page-header">
                        <div class="album-page-album-info">
                            <Show when={album()}>
                                {(album) => (
                                    <>
                                        <div class="album-page-album-info-artwork">
                                            <Album
                                                album={album()}
                                                artist={false}
                                                title={false}
                                                size={300}
                                                route={false}
                                                onClick={showArtwork}
                                            />
                                        </div>
                                        <div class="album-page-album-info-details">
                                            <div class="album-page-album-info-details-album-title">
                                                {album().title}
                                            </div>
                                            <div class="album-page-album-info-details-album-artist">
                                                <a
                                                    href={artistRoute(album())}
                                                    class="album-page-album-info-details-album-artist-text"
                                                >
                                                    {album().artist}
                                                </a>
                                            </div>
                                            <div class="album-page-album-info-details-tracks">
                                                <Show when={getTracks()}>
                                                    {(tracks) => (
                                                        <>
                                                            {tracks().length}{' '}
                                                            tracks (
                                                            {toTime(
                                                                Math.round(
                                                                    albumDuration(),
                                                                ),
                                                            )}
                                                            )
                                                        </>
                                                    )}
                                                </Show>
                                            </div>
                                            <div class="album-page-album-info-details-release-date">
                                                {displayDate(
                                                    album().dateReleased,
                                                    'LLLL dd, yyyy',
                                                )}
                                            </div>
                                            <div
                                                class={`album-page-album-info-details-versions${
                                                    (versions()?.length ?? 0) >
                                                    1
                                                        ? ' multiple'
                                                        : ''
                                                }`}
                                            >
                                                <For each={versions()}>
                                                    {(version, index) => (
                                                        <>
                                                            <span
                                                                class={`album-page-album-info-details-versions-version${
                                                                    version ===
                                                                    activeVersion()
                                                                        ? ' active'
                                                                        : ''
                                                                }`}
                                                                onClick={() =>
                                                                    setActiveVersion(
                                                                        version,
                                                                    )
                                                                }
                                                            >
                                                                {displayAlbumVersionQuality(
                                                                    version,
                                                                )}
                                                            </span>
                                                            <>
                                                                {index() <
                                                                    versions()!
                                                                        .length -
                                                                        1 && (
                                                                    <span>
                                                                        {' '}
                                                                        /{' '}
                                                                    </span>
                                                                )}
                                                            </>
                                                        </>
                                                    )}
                                                </For>
                                            </div>
                                        </div>
                                    </>
                                )}
                            </Show>
                        </div>
                        <div class="album-page-album-controls">
                            <div class="album-page-album-controls-playback">
                                <button
                                    class="album-page-album-controls-playback-play-button"
                                    onClick={(e) => {
                                        e.stopPropagation();
                                        e.preventDefault();
                                        if (getTracks()) {
                                            playPlaylist(getTracks()!);
                                        }
                                        return false;
                                    }}
                                >
                                    <img
                                        src="/img/play-button.svg"
                                        alt="Play"
                                    />{' '}
                                    Play
                                </button>
                                <button
                                    class="album-page-album-controls-playback-options-button"
                                    onClick={(e) => {
                                        e.stopPropagation();
                                        e.preventDefault();
                                        if (getTracks()) {
                                            addTracksToQueue(getTracks()!);
                                        }
                                        return false;
                                    }}
                                >
                                    <img
                                        src="/img/more-options.svg"
                                        alt="Options"
                                    />{' '}
                                    Options
                                </button>
                                <Show
                                    when={album()?.albumSources?.every(
                                        (x) => x.source !== 'LIBRARY',
                                    )}
                                >
                                    <button
                                        class="album-page-album-controls-playback-add-to-library-button"
                                        onClick={(e) => {
                                            e.stopPropagation();
                                            e.preventDefault();
                                            addAlbumToLibrary();
                                            return false;
                                        }}
                                    >
                                        Add to Library
                                    </button>
                                </Show>
                                <Show
                                    when={
                                        album()?.albumSources.some(
                                            (x) => x.source === 'TIDAL',
                                        ) &&
                                        activeVersion()?.source ===
                                            Api.TrackSource.TIDAL &&
                                        album()?.albumSources.some(
                                            ({ source }) =>
                                                source === 'LIBRARY',
                                        )
                                    }
                                >
                                    <button
                                        class="album-page-album-controls-playback-remove-from-library-button"
                                        onClick={(e) => {
                                            e.stopPropagation();
                                            e.preventDefault();
                                            removeAlbumFromLibrary({
                                                tidalAlbumId:
                                                    album()!.albumSources.find(
                                                        (x) =>
                                                            x.source ===
                                                            'TIDAL',
                                                    )?.id,
                                            });
                                            return false;
                                        }}
                                    >
                                        Remove from Library
                                    </button>
                                </Show>
                                <Show
                                    when={
                                        album()?.albumSources.some(
                                            (x) => x.source === 'QOBUZ',
                                        ) &&
                                        activeVersion()?.source ===
                                            Api.TrackSource.QOBUZ &&
                                        album()?.albumSources.some(
                                            ({ source }) =>
                                                source === 'LIBRARY',
                                        )
                                    }
                                >
                                    <button
                                        class="album-page-album-controls-playback-remove-from-library-button"
                                        onClick={(e) => {
                                            e.stopPropagation();
                                            e.preventDefault();
                                            removeAlbumFromLibrary({
                                                qobuzAlbumId:
                                                    album()!.albumSources.find(
                                                        (x) =>
                                                            x.source ===
                                                            'QOBUZ',
                                                    )?.id,
                                            });
                                            return false;
                                        }}
                                    >
                                        Remove from Library
                                    </button>
                                </Show>
                                <Show
                                    when={
                                        props.tidalAlbumId &&
                                        isInvalidFavorite()
                                    }
                                >
                                    <button
                                        class="album-page-album-controls-playback-refavorite-button"
                                        onClick={(e) => {
                                            e.stopPropagation();
                                            e.preventDefault();
                                            refavoriteAlbum({
                                                tidalAlbumId:
                                                    album()!.albumSources.find(
                                                        (x) =>
                                                            x.source ===
                                                            'TIDAL',
                                                    )?.id,
                                            });
                                            return false;
                                        }}
                                    >
                                        Re-favorite Tidal Album
                                    </button>
                                </Show>
                                <Show
                                    when={
                                        props.qobuzAlbumId &&
                                        isInvalidFavorite()
                                    }
                                >
                                    <button
                                        class="album-page-album-controls-playback-refavorite-button"
                                        onClick={(e) => {
                                            e.stopPropagation();
                                            e.preventDefault();
                                            refavoriteAlbum({
                                                qobuzAlbumId:
                                                    album()!.albumSources.find(
                                                        (x) =>
                                                            x.source ===
                                                            'QOBUZ',
                                                    )?.id,
                                            });
                                            return false;
                                        }}
                                    >
                                        Re-favorite Qobuz Album
                                    </button>
                                </Show>
                                <Show
                                    when={
                                        activeVersion()?.source ===
                                            Api.TrackSource.TIDAL &&
                                        album()?.albumSources.some(
                                            ({ source }) =>
                                                source === 'LIBRARY',
                                        )
                                    }
                                >
                                    <button
                                        class="album-page-album-controls-playback-download-button"
                                        onClick={(e) => {
                                            e.stopPropagation();
                                            e.preventDefault();
                                            downloadAlbum('TIDAL');
                                            return false;
                                        }}
                                    >
                                        Download album
                                    </button>
                                </Show>
                                <Show
                                    when={
                                        activeVersion()?.source ===
                                            Api.TrackSource.QOBUZ &&
                                        album()?.albumSources.some(
                                            ({ source }) =>
                                                source === 'LIBRARY',
                                        )
                                    }
                                >
                                    <button
                                        class="album-page-album-controls-playback-download-button"
                                        onClick={(e) => {
                                            e.stopPropagation();
                                            e.preventDefault();
                                            downloadAlbum('QOBUZ');
                                            return false;
                                        }}
                                    >
                                        Download album
                                    </button>
                                </Show>
                            </div>
                            <div class="album-page-album-controls-options"></div>
                        </div>
                    </div>
                    <table class="album-page-tracks">
                        <thead>
                            <tr>
                                <th class="album-page-tracks-track-no-header">
                                    #
                                </th>
                                <th>Title</th>
                                <th class="album-page-tracks-artist-header">
                                    Artist
                                </th>
                                <th class="album-page-tracks-track-time-header">
                                    Time
                                </th>
                                <th></th>
                            </tr>
                        </thead>
                        <tbody>
                            <Show when={getTracks()}>
                                <For each={getTracks()!}>{track}</For>
                            </Show>
                        </tbody>
                    </table>
                </div>
            </div>
            <Show when={showingArtwork()}>{albumArtworkPreviewer()}</Show>
        </div>
    );
}
