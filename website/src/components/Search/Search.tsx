import './search.css';
import { For, Show, createSignal, onCleanup, onMount } from 'solid-js';
import type { JSXElement } from 'solid-js';
import { debounce } from '@solid-primitives/scheduled';
import {
    Api,
    api,
    type ApiSource,
    type Album as ApiAlbum,
    type Artist as ApiArtist,
    once,
} from '~/services/api';
import Artist from '../Artist';
import Album from '../Album';
import { isServer } from 'solid-js/web';
import { artistRoute } from '../Artist/Artist';
import { albumRoute } from '../Album/Album';
import { displayApiSource } from '~/services/formatting';
import Tabs from '../Tabs';

export default function searchInput() {
    let searchContainerRef: HTMLDivElement;
    let searchInputRef: HTMLInputElement;
    let searchResultsRef: HTMLDivElement;

    const [libraryLoading, setLibraryLoading] = createSignal(false);
    const [qobuzLoading, setQobuzLoading] = createSignal(false);
    const [tidalLoading, setTidalLoading] = createSignal(false);
    const [ytLoading, setYtLoading] = createSignal(false);
    const [searchFilterValue, setSearchFilterValue] = createSignal('');
    const [searchResults, setSearchResults] =
        createSignal<Api.GlobalSearchResult[]>();
    const [qobuzSearchResults, setQobuzSearchResults] =
        createSignal<Api.GlobalSearchResult[]>();
    const [tidalSearchResults, setTidalSearchResults] =
        createSignal<Api.GlobalSearchResult[]>();
    const [ytSearchResults, setYtSearchResults] =
        createSignal<Api.GlobalSearchResult[]>();

    function closeSearch() {
        searchInputRef.focus();
        searchInputRef.blur();
    }

    function inputFocused(
        e: FocusEvent & {
            currentTarget: HTMLInputElement;
            target: HTMLInputElement;
        },
    ) {
        e.target.select();
    }

    onMount(() => {
        if (isServer) return;
    });

    onCleanup(() => {
        if (isServer) return;
    });

    function searchResultToApiArtist(
        source: ApiSource,
        result: Api.GlobalArtistSearchResult | Api.GlobalTrackSearchResult,
    ): ApiArtist {
        switch (source) {
            case 'LIBRARY':
                return {
                    ...result,
                    type: 'LIBRARY',
                    artistId: result.artistId as number,
                };
            case 'TIDAL':
                return {
                    ...result,
                    type: 'TIDAL',
                    id: result.artistId as number,
                };
            case 'QOBUZ':
                return {
                    ...result,
                    type: 'QOBUZ',
                    id: result.artistId as number,
                };
            case 'YT':
                return {
                    ...result,
                    type: 'YT',
                    id: result.artistId as string,
                };
            default:
                source satisfies never;
                throw new Error(`Invalid ApiSource: "${source}"`);
        }
    }

    function searchResultToApiAlbum(
        source: ApiSource,
        result: Api.GlobalAlbumSearchResult | Api.GlobalTrackSearchResult,
    ): ApiAlbum {
        switch (source) {
            case 'LIBRARY':
                return {
                    ...result,
                    type: 'LIBRARY',
                    artistId: result.artistId as number,
                    albumId: result.albumId as number,
                    versions: [],
                };
            case 'TIDAL':
                return {
                    ...result,
                    type: 'TIDAL',
                    artistId: result.artistId as number,
                    id: result.albumId as number,
                    explicit: false,
                    numberOfTracks: 0,
                    audioQuality: 'LOSSLESS',
                    mediaMetadataTags: [],
                };
            case 'QOBUZ':
                return {
                    ...result,
                    type: 'QOBUZ',
                    artistId: result.artistId as number,
                    id: result.albumId as string,
                    parentalWarning: false,
                    numberOfTracks: 0,
                };
            case 'YT':
                return {
                    ...result,
                    type: 'YT',
                    artistId: result.artistId as string,
                    id: result.albumId as string,
                    numberOfTracks: 0,
                };
            default:
                source satisfies never;
                throw new Error(`Invalid ApiSource: "${source}"`);
        }
    }

    async function search(searchString: string) {
        setSearchFilterValue(searchString);

        if (!searchString.trim()) return;

        searchResultsRef.scroll({ top: 0, behavior: 'instant' });

        try {
            setLibraryLoading(true);
            setQobuzLoading(true);
            setTidalLoading(true);
            setYtLoading(true);
            once('search', async (signal) => {
                await api.searchAll(
                    searchString,
                    0,
                    20,
                    (results, _allResults, source) => {
                        switch (source) {
                            case 'LIBRARY':
                                setSearchResults(results);
                                setLibraryLoading(false);
                                break;
                            case 'QOBUZ':
                                setQobuzSearchResults(results);
                                setQobuzLoading(false);
                                break;
                            case 'TIDAL':
                                setTidalSearchResults(results);
                                setTidalLoading(false);
                                break;
                            case 'YT':
                                setYtSearchResults(results);
                                setYtLoading(false);
                                break;
                            default:
                                console.log(
                                    `received ${source} results:`,
                                    results,
                                );
                        }
                    },
                    signal,
                );
            });
        } catch (e) {
            console.error('Failed to run global search', e);
        }
    }

    function searchResultLink(
        source: ApiSource,
        result: Api.GlobalSearchResult,
    ): string {
        const resultType = result.type;

        switch (resultType) {
            case 'ARTIST':
                return artistRoute({ id: result.artistId, type: source });
            case 'ALBUM':
                return albumRoute({ id: result.albumId, type: source });
            case 'TRACK':
                return albumRoute({ id: result.albumId, type: source });
            default:
                resultType satisfies never;
                throw new Error(`Invalid result type: ${resultType}`);
        }
    }

    function searchResult(
        source: ApiSource,
        result: Api.GlobalSearchResult,
    ): JSXElement {
        switch (result.type) {
            case 'ARTIST': {
                const artist = result as Api.GlobalArtistSearchResult;
                const apiArtist = searchResultToApiArtist(source, artist);
                return (
                    <div class="search-results-result">
                        <div class="search-results-result-icon">
                            <Artist
                                size={50}
                                artist={apiArtist}
                                route={false}
                            />
                        </div>
                        <div class="search-results-result-details">
                            <span class="search-results-result-details-type">
                                Artist
                            </span>{' '}
                            <span class="search-results-result-details-stop-word">
                                -
                            </span>{' '}
                            <a
                                href={artistRoute({
                                    id: artist.artistId,
                                    type: source,
                                })}
                                class="search-results-result-details-artist"
                                tabindex="-1"
                            >
                                {artist.title}
                            </a>
                        </div>
                    </div>
                );
            }
            case 'ALBUM': {
                const album = result as Api.GlobalAlbumSearchResult;
                const apiAlbum = searchResultToApiAlbum(source, album);
                return (
                    <div class="search-results-result">
                        <div class="search-results-result-icon">
                            <Album
                                size={50}
                                artist={false}
                                year={false}
                                route={false}
                                album={apiAlbum}
                            />
                        </div>
                        <div class="search-results-result-details">
                            <span class="search-results-result-details-type">
                                Album
                            </span>{' '}
                            <span class="search-results-result-details-stop-word">
                                -
                            </span>{' '}
                            <a
                                href={albumRoute({
                                    id: album.albumId,
                                    type: source,
                                })}
                                class="search-results-result-details-album"
                                tabindex="-1"
                            >
                                {album.title}
                            </a>{' '}
                            <span class="search-results-result-details-stop-word">
                                by
                            </span>{' '}
                            <a
                                href={artistRoute({
                                    id: album.artistId,
                                    type: source,
                                })}
                                class="search-results-result-details-artist"
                                tabindex="-1"
                            >
                                {album.artist}
                            </a>
                        </div>
                    </div>
                );
            }
            case 'TRACK': {
                const track = result as Api.GlobalTrackSearchResult;
                const apiAlbum = searchResultToApiAlbum(source, track);
                return (
                    <div class="search-results-result">
                        <div class="search-results-result-icon">
                            <Album
                                size={50}
                                artist={false}
                                year={false}
                                route={false}
                                album={apiAlbum}
                            />
                        </div>
                        <div class="search-results-result-details">
                            <span class="search-results-result-details-type">
                                Track
                            </span>{' '}
                            <span class="search-results-result-details-stop-word">
                                -
                            </span>{' '}
                            <a
                                href={albumRoute({
                                    id: track.albumId,
                                    type: source,
                                })}
                                class="search-results-result-details-track"
                                tabindex="-1"
                            >
                                {track.title}
                            </a>{' '}
                            <span class="search-results-result-details-stop-word">
                                on
                            </span>{' '}
                            <a
                                href={albumRoute({
                                    id: track.albumId,
                                    type: source,
                                })}
                                class="search-results-result-details-album"
                                tabindex="-1"
                            >
                                {track.album}
                            </a>{' '}
                            <span class="search-results-result-details-stop-word">
                                by
                            </span>{' '}
                            <a
                                href={artistRoute({
                                    id: track.artistId,
                                    type: source,
                                })}
                                class="search-results-result-details-artist"
                                tabindex="-1"
                            >
                                {track.artist}
                            </a>
                        </div>
                    </div>
                );
            }
        }
    }

    function searchResultsList(
        source: ApiSource,
        loading: boolean,
        results?: Api.GlobalSearchResult[],
    ): JSXElement {
        return (
            <div
                class={`search-results-list${loading ? ' loading' : ' loaded'}`}
            >
                <Show when={results?.length === 0}>No results</Show>
                <Show when={(results?.length ?? 0) !== 0}>
                    <For each={results}>
                        {(result) => (
                            <a
                                href={searchResultLink(source, result)}
                                class="search-results-result-link"
                                onClick={() => closeSearch()}
                            >
                                {searchResult(source, result)}
                            </a>
                        )}
                    </For>
                </Show>
                <Show when={loading}>Loading...</Show>
            </div>
        );
    }

    return (
        <div
            data-turbo-permanent
            id="search-bar"
            class="search-container"
            ref={searchContainerRef!}
        >
            <div class="search-label-container">
                <label class="search-label">
                    <input
                        ref={searchInputRef!}
                        class="search-input"
                        title="Search..."
                        type="text"
                        onFocus={(e) => inputFocused(e)}
                        value={searchFilterValue()}
                        onInput={debounce(async (e) => {
                            await search(e.target.value ?? '');
                        }, 200)}
                        onKeyUp={(e) => e.key === 'Escape' && closeSearch()}
                    />
                    <div class="search-backdrop"></div>
                </label>
                <img
                    src={'/img/cross.svg'}
                    class="cancel-search-icon"
                    onClick={(e) => {
                        e.stopPropagation();
                        closeSearch();
                    }}
                />
            </div>
            <div
                class="search-results"
                style={{
                    display: searchFilterValue()?.trim() ? undefined : 'none',
                }}
                ref={searchResultsRef!}
            >
                <Tabs
                    default={'LIBRARY'}
                    tabs={{
                        LIBRARY: displayApiSource('LIBRARY'),
                        QOBUZ: displayApiSource('QOBUZ'),
                        TIDAL: displayApiSource('TIDAL'),
                        YT: displayApiSource('YT'),
                    }}
                >
                    {(tab) => {
                        switch (tab) {
                            case 'LIBRARY':
                                return searchResultsList(
                                    'LIBRARY',
                                    libraryLoading(),
                                    searchResults(),
                                );
                            case 'QOBUZ':
                                return searchResultsList(
                                    'QOBUZ',
                                    qobuzLoading(),
                                    qobuzSearchResults(),
                                );
                            case 'TIDAL':
                                return searchResultsList(
                                    'TIDAL',
                                    tidalLoading(),
                                    tidalSearchResults(),
                                );
                            case 'YT':
                                return searchResultsList(
                                    'YT',
                                    ytLoading(),
                                    ytSearchResults(),
                                );
                            default:
                                throw new Error(`Invalid tab: ${tab}`);
                        }
                    }}
                </Tabs>
            </div>
        </div>
    );
}
