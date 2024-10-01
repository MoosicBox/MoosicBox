import './artist-page.css';
import { createEffect, createSignal, For, on, Show } from 'solid-js';
import Album from '~/components/Album';
import Artist from '~/components/Artist';
import { Api, api, type Artist as ApiArtist } from '~/services/api';
import { historyBack } from '~/services/util';

export default function artistPage(props: {
    artistId?: number;
    tidalArtistId?: number;
    qobuzArtistId?: number;
}) {
    const [libraryArtist, setLibraryArtist] =
        createSignal<Api.LibraryArtist | null>();
    const [libraryAlbums, setLibraryAlbums] = createSignal<
        Api.LibraryAlbum[] | null
    >();

    const [tidalArtist, setTidalArtist] = createSignal<Api.TidalArtist>();
    const [tidalAlbums, setTidalAlbums] = createSignal<Api.TidalAlbum[]>();
    const [tidalEpsAndSingles, setTidalEpsAndSingles] =
        createSignal<Api.TidalAlbum[]>();
    const [tidalCompilations, setTidalCompilations] =
        createSignal<Api.TidalAlbum[]>();

    const [qobuzArtist, setQobuzArtist] = createSignal<Api.QobuzArtist>();
    const [qobuzAlbums, setQobuzAlbums] = createSignal<Api.QobuzAlbum[]>();
    const [qobuzEpsAndSingles, setQobuzEpsAndSingles] =
        createSignal<Api.QobuzAlbum[]>();
    const [qobuzCompilations, setQobuzCompilations] =
        createSignal<Api.QobuzAlbum[]>();

    function getArtist(): ApiArtist | null | undefined {
        return libraryArtist() ?? tidalArtist() ?? qobuzArtist();
    }

    async function loadQobuzAlbums(qobuzId: number) {
        await Promise.all([
            api.getAllQobuzArtistAlbums(qobuzId, setQobuzAlbums, ['LP']),
            api.getAllQobuzArtistAlbums(qobuzId, setQobuzEpsAndSingles, [
                'EPS_AND_SINGLES',
            ]),
            api.getAllQobuzArtistAlbums(qobuzId, setQobuzCompilations, [
                'COMPILATIONS',
            ]),
        ]);
    }

    async function loadTidalAlbums(tidalId: number) {
        await Promise.all([
            api.getAllTidalArtistAlbums(tidalId, setTidalAlbums, ['LP']),
            api.getAllTidalArtistAlbums(tidalId, setTidalEpsAndSingles, [
                'EPS_AND_SINGLES',
            ]),
            api.getAllTidalArtistAlbums(tidalId, setTidalCompilations, [
                'COMPILATIONS',
            ]),
        ]);
    }

    async function loadLibraryArtist(): Promise<Api.LibraryArtist | undefined> {
        if (props.artistId) {
            const artist = await api.getArtist(props.artistId);
            setLibraryArtist(artist);
            return artist;
        } else if (props.tidalArtistId) {
            const artist = await api.getArtistFromTidalArtistId(
                props.tidalArtistId,
            );
            setLibraryArtist(artist);

            if (artist.qobuzId) {
                loadQobuzAlbums(artist.qobuzId);
            }

            return artist;
        } else if (props.qobuzArtistId) {
            const artist = await api.getArtistFromQobuzArtistId(
                props.qobuzArtistId,
            );
            setLibraryArtist(artist);

            if (artist.tidalId) {
                loadTidalAlbums(artist.tidalId);
            }

            return artist;
        }

        return undefined;
    }

    async function loadTidalArtist(
        tidalArtistId: number,
    ): Promise<Api.TidalArtist | undefined> {
        const tidalArtist = await api.getTidalArtist(tidalArtistId);
        setTidalArtist(tidalArtist);
        return tidalArtist;
    }

    async function loadQobuzArtist(
        qobuzArtistId: number,
    ): Promise<Api.QobuzArtist | undefined> {
        const qobuzArtist = await api.getQobuzArtist(qobuzArtistId);
        setQobuzArtist(qobuzArtist);
        return qobuzArtist;
    }

    async function loadArtist() {
        const promises = [];
        let loadedArtist = false;

        if (props.artistId) {
            const artist = await loadLibraryArtist();
            loadedArtist = true;

            if (artist?.tidalId) {
                promises.push(loadTidalAlbums(artist.tidalId));
            }
            if (artist?.qobuzId) {
                promises.push(loadQobuzAlbums(artist.qobuzId));
            }
        }
        if (props.tidalArtistId) {
            promises.push(loadTidalArtist(props.tidalArtistId));
        }
        if (props.qobuzArtistId) {
            promises.push(loadQobuzArtist(props.qobuzArtistId));
        }

        if (!loadedArtist) {
            promises.push(loadLibraryArtist());
        }

        await Promise.all(promises);
    }

    async function loadLibraryAlbums() {
        try {
            if (props.artistId) {
                const albums = await api.getAllAlbums({
                    artistId: props.artistId,
                    sort: 'Release-Date-Desc',
                });
                setLibraryAlbums(albums);
            } else if (props.tidalArtistId) {
                const libraryAlbum = await api.getAllAlbums({
                    tidalArtistId: props.tidalArtistId,
                    sort: 'Release-Date-Desc',
                });
                setLibraryAlbums(libraryAlbum);
            } else if (props.qobuzArtistId) {
                const libraryAlbum = await api.getAllAlbums({
                    qobuzArtistId: props.qobuzArtistId,
                    sort: 'Release-Date-Desc',
                });
                setLibraryAlbums(libraryAlbum);
            }
        } catch {
            setLibraryAlbums(null);
        }
    }

    async function loadAlbums() {
        if (props.artistId) {
            await loadLibraryAlbums();
        }
        if (props.tidalArtistId) {
            await Promise.all([
                loadLibraryAlbums(),
                loadTidalAlbums(props.tidalArtistId),
            ]);
        }
        if (props.qobuzArtistId) {
            await Promise.all([
                loadLibraryAlbums(),
                loadQobuzAlbums(props.qobuzArtistId),
            ]);
        }
    }

    createEffect(
        on(
            () => props.artistId,
            (value, prev) => {
                if (value !== prev) loadPage();
            },
        ),
    );

    createEffect(
        on(
            () => props.tidalArtistId,
            (value, prev) => {
                if (value !== prev) loadPage();
            },
        ),
    );

    createEffect(
        on(
            () => props.qobuzArtistId,
            (value, prev) => {
                if (value !== prev) loadPage();
            },
        ),
    );

    async function loadPage() {
        await Promise.all([loadArtist(), loadAlbums()]);
    }

    return (
        <>
            <div class="artist-page-container">
                <div class="artist-page">
                    <div class="artist-page-breadcrumbs">
                        <a
                            class="back-button"
                            href="#"
                            onClick={() => historyBack()}
                        >
                            Back
                        </a>
                    </div>
                    <div class="artist-page-header">
                        <div class="artist-page-artist-info">
                            <div class="artist-page-artist-info-cover">
                                <Show when={getArtist()}>
                                    {(artist) => (
                                        <Artist
                                            artist={artist()}
                                            route={false}
                                            size={400}
                                        />
                                    )}
                                </Show>
                            </div>
                            <div class="artist-page-artist-info-details">
                                <h1 class="artist-page-artist-info-details-artist-title">
                                    {getArtist()?.title}
                                </h1>
                            </div>
                        </div>
                    </div>
                    <Show when={(libraryAlbums()?.length ?? 0) > 0}>
                        <h1 class="artist-page-albums-header">
                            Albums in Library
                        </h1>
                        <div class="artist-page-albums">
                            <For each={libraryAlbums()}>
                                {(album) => (
                                    <Album
                                        album={album}
                                        artist={true}
                                        title={true}
                                        year={true}
                                        controls={true}
                                        versionQualities={true}
                                        size={200}
                                    />
                                )}
                            </For>
                        </div>
                    </Show>
                    <Show when={(tidalAlbums()?.length ?? 0) > 0}>
                        <h1 class="artist-page-albums-header">
                            Albums on Tidal
                        </h1>
                        <div class="artist-page-albums">
                            <For each={tidalAlbums()}>
                                {(album) => (
                                    <Album
                                        album={album}
                                        artist={true}
                                        title={true}
                                        year={true}
                                        controls={true}
                                        versionQualities={true}
                                        size={200}
                                    />
                                )}
                            </For>
                        </div>
                    </Show>
                    <Show when={(tidalEpsAndSingles()?.length ?? 0) > 0}>
                        <h1 class="artist-page-albums-header">
                            EPs and Singles on Tidal
                        </h1>
                        <div class="artist-page-albums">
                            <For each={tidalEpsAndSingles()}>
                                {(album) => (
                                    <Album
                                        album={album}
                                        artist={true}
                                        title={true}
                                        year={true}
                                        controls={true}
                                        versionQualities={true}
                                        size={200}
                                    />
                                )}
                            </For>
                        </div>
                    </Show>
                    <Show when={(tidalCompilations()?.length ?? 0) > 0}>
                        <h1 class="artist-page-albums-header">
                            Compilations on Tidal
                        </h1>
                        <div class="artist-page-albums">
                            <For each={tidalCompilations()}>
                                {(album) => (
                                    <Album
                                        album={album}
                                        artist={true}
                                        title={true}
                                        year={true}
                                        controls={true}
                                        versionQualities={true}
                                        size={200}
                                    />
                                )}
                            </For>
                        </div>
                    </Show>
                    <Show when={(qobuzAlbums()?.length ?? 0) > 0}>
                        <h1 class="artist-page-albums-header">
                            Albums on Qobuz
                        </h1>
                        <div class="artist-page-albums">
                            <For each={qobuzAlbums()}>
                                {(album) => (
                                    <Album
                                        album={album}
                                        artist={true}
                                        title={true}
                                        year={true}
                                        controls={true}
                                        versionQualities={true}
                                        size={200}
                                    />
                                )}
                            </For>
                        </div>
                    </Show>
                    <Show when={(qobuzEpsAndSingles()?.length ?? 0) > 0}>
                        <h1 class="artist-page-albums-header">
                            EPs and Singles on Qobuz
                        </h1>
                        <div class="artist-page-albums">
                            <For each={qobuzEpsAndSingles()}>
                                {(album) => (
                                    <Album
                                        album={album}
                                        artist={true}
                                        title={true}
                                        year={true}
                                        controls={true}
                                        versionQualities={true}
                                        size={200}
                                    />
                                )}
                            </For>
                        </div>
                    </Show>
                    <Show when={(qobuzCompilations()?.length ?? 0) > 0}>
                        <h1 class="artist-page-albums-header">
                            Compilations on Qobuz
                        </h1>
                        <div class="artist-page-albums">
                            <For each={qobuzCompilations()}>
                                {(album) => (
                                    <Album
                                        album={album}
                                        artist={true}
                                        title={true}
                                        year={true}
                                        controls={true}
                                        versionQualities={true}
                                        size={200}
                                    />
                                )}
                            </For>
                        </div>
                    </Show>
                </div>
            </div>
        </>
    );
}
