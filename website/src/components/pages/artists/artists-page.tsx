import './artists-page.css';
import {
    createComputed,
    createSignal,
    For,
    onCleanup,
    onMount,
    Show,
} from 'solid-js';
import { isServer } from 'solid-js/web';
import { debounce } from '@solid-primitives/scheduled';
import { api, Api, once } from '~/services/api';
import { currentArtistSearch, setCurrentArtistSearch } from '~/services/app';
import Artist from '~/components/Artist';
import { QueryParams } from '~/services/util';

let historyListener: () => void;

export default function artists() {
    let backToTopRef: HTMLDivElement;
    let artistSortControlsRef: HTMLDivElement | undefined;
    let artistsHeaderContainerRef: HTMLDivElement;

    const [loading, setLoading] = createSignal(false);
    const [artists, setArtists] = createSignal<Api.LibraryArtist[]>();
    const [searchFilterValue, setSearchFilterValue] = createSignal<string>();
    const [currentArtistSort, setCurrentArtistSort] =
        createSignal<Api.ArtistSort>('Name');
    const [showArtistSortControls, setShowArtistSortControls] =
        createSignal(false);
    const searchParams = new QueryParams(
        isServer ? {} : window.location.search,
    );

    createComputed(() => {
        if (searchParams.has('sort')) {
            setCurrentArtistSort(searchParams.get('sort') as Api.ArtistSort);
        }
    });

    function setSearchParam(name: string, value: string) {
        searchParams.set(name, value);
        const newRelativePathQuery = `${window.location.pathname}?${searchParams}`;
        history.pushState(null, '', newRelativePathQuery);

        if (name === 'sort') {
            setCurrentArtistSort(value as Api.ArtistSort);
        }
    }

    function removeSearchParam(name: string) {
        searchParams.delete(name);
        const newRelativePathQuery = `${window.location.pathname}?${searchParams}`;
        history.pushState(null, '', newRelativePathQuery);
    }

    function getArtistSources(): Api.AlbumSource[] | undefined {
        return searchParams.get('sources')?.split(',') as
            | Api.AlbumSource[]
            | undefined;
    }

    function getArtistSort(): Api.ArtistSort | undefined {
        return searchParams.get('sort') as Api.ArtistSort | undefined;
    }

    function getSearchFilter(): string | undefined {
        return searchParams.get('search') as string | undefined;
    }

    function setArtistSources(sources: Api.AlbumSource[]) {
        setSearchParam('sources', sources.join(','));
    }

    function setArtistSort(sort: Api.ArtistSort) {
        setSearchParam('sort', sort);
    }

    function setSearchFilter(search: string) {
        if (search.trim().length === 0) {
            removeSearchParam('search');
        } else {
            setSearchParam('search', search);
        }
        setSearchFilterValue(search);
    }

    async function loadArtists(
        request: Api.ArtistsRequest | undefined = undefined,
    ) {
        const query = searchParams.toString();
        if (!artists()) {
            const current = currentArtistSearch();
            if (current && current.query === query) {
                setArtists(current.results);
                return;
            }
        }
        if (request?.sources) setArtistSources(request.sources);
        if (request?.sort) setArtistSort(request.sort);
        if (typeof request?.filters?.search === 'string')
            setSearchFilter(request.filters.search);

        try {
            setLoading(true);
            setArtists(
                await once('artists', (signal) =>
                    api.getArtists(
                        {
                            sources: getArtistSources(),
                            sort: getArtistSort(),
                            filters: {
                                search: getSearchFilter(),
                            },
                        },
                        signal,
                    ),
                ),
            );
        } catch (e) {
            console.error('Failed to fetch artists', e);
            setArtists(undefined);
        } finally {
            setLoading(false);
        }

        const results = artists();

        if (results) {
            setCurrentArtistSearch({ query, results });
        }
    }

    if (!isServer) {
        if (historyListener) {
            window.removeEventListener('popstate', historyListener);
        }

        historyListener = () => {
            const newSearchParams = new QueryParams(window.location.search);

            let wasChange = false;

            searchParams.forEach((_value, key) => {
                if (!newSearchParams.has(key)) {
                    switch (key) {
                        case 'sources':
                            wasChange = true;
                            break;
                        case 'sort':
                            wasChange = true;
                            break;
                        case 'search':
                            searchParams.delete(key);
                            setSearchFilterValue('');
                            wasChange = true;
                            break;
                    }
                }
            });

            newSearchParams.forEach((value, key) => {
                if (searchParams.get(key) !== value) {
                    searchParams.set(key, value);

                    switch (key) {
                        case 'sources':
                            wasChange = true;
                            break;
                        case 'sort':
                            wasChange = true;
                            break;
                        case 'search':
                            setSearchFilterValue(value);
                            wasChange = true;
                            break;
                    }
                }
            });

            if (wasChange) {
                loadArtists();
            }
        };

        window.addEventListener('popstate', historyListener);
    }

    onCleanup(() => {
        if (historyListener) {
            window.removeEventListener('popstate', historyListener);
        }
    });

    const handleArtistSortClick = (_event: MouseEvent) => {
        if (!showArtistSortControls()) return;
        setShowArtistSortControls(false);
    };

    onMount(() => {
        if (isServer) return;
        window.addEventListener('click', handleArtistSortClick);
    });

    onCleanup(() => {
        if (isServer) return;
        window.removeEventListener('click', handleArtistSortClick);
    });

    function showBackToTop() {
        if (backToTopRef.style.display === 'block') return;
        clearTimeout(backToTopTimeout);
        backToTopRef.style.opacity = '0';
        backToTopRef.style.display = 'block';
        backToTopTimeout = setTimeout(() => {
            backToTopRef.style.opacity = '1';
        }, 0);
    }

    function hideBackToTop() {
        if (backToTopRef.style.opacity === '0') return;
        clearTimeout(backToTopTimeout);
        backToTopRef.style.opacity = '0';
        backToTopTimeout = setTimeout(() => {
            backToTopRef.style.display = 'none';
        }, 300);
    }

    let backToTopTimeout: NodeJS.Timeout;
    const scrollListener = () => {
        if (
            (document.querySelector('main')?.scrollTop ?? 0) >
            artistsHeaderContainerRef.getBoundingClientRect().bottom
        ) {
            showBackToTop();
        } else {
            hideBackToTop();
        }
    };

    onMount(() => {
        if (isServer) return;
        document
            .querySelector('main')
            ?.addEventListener('scroll', scrollListener);

        scrollListener();
    });

    onCleanup(() => {
        if (isServer) return;
        document
            .querySelector('main')
            ?.removeEventListener('scroll', scrollListener);
    });

    onMount(async () => {
        if (isServer) return;
        setSearchFilterValue(getSearchFilter() ?? '');
        await loadArtists();
    });

    return (
        <>
            <div class="artists-back-to-top-container main-content-back-to-top">
                <div
                    onClick={() =>
                        document.querySelector('main')?.scroll({
                            top: 0,
                            behavior: 'smooth',
                        })
                    }
                    class="artists-back-to-top"
                    ref={backToTopRef!}
                >
                    <div class="artists-back-to-top-content">
                        <img
                            class="artists-back-to-top-chevron"
                            src="/img/chevron-up-white.svg"
                        />
                        Back to top
                        <img
                            class="artists-back-to-top-chevron"
                            src="/img/chevron-up-white.svg"
                        />
                    </div>
                </div>
            </div>
            <header
                class="artists-header-container"
                ref={artistsHeaderContainerRef!}
            >
                <div class="artists-header-backdrop"></div>
                <div class="artists-header-text-container">
                    <h1 class="artists-header-text">
                        Artists{' '}
                        <img
                            class="artists-header-sort-icon"
                            src="/img/more-options-white.svg"
                            onClick={(event) => {
                                setShowArtistSortControls(
                                    !showArtistSortControls(),
                                );
                                event.stopPropagation();
                            }}
                        />
                    </h1>
                    {showArtistSortControls() && (
                        <div
                            class="artists-sort-controls"
                            ref={artistSortControlsRef!}
                        >
                            <div
                                onClick={() =>
                                    loadArtists({
                                        sort:
                                            getArtistSort() === 'Name-Desc'
                                                ? 'Name'
                                                : 'Name-Desc',
                                    })
                                }
                            >
                                Artist Name
                                {currentArtistSort() === 'Name' && (
                                    <img
                                        class="sort-chevron-icon"
                                        src="/img/chevron-up-white.svg"
                                    />
                                )}
                                {currentArtistSort() === 'Name-Desc' && (
                                    <img
                                        class="sort-chevron-icon"
                                        src="/img/chevron-down-white.svg"
                                    />
                                )}
                            </div>
                        </div>
                    )}
                </div>
                <input
                    class="filter-artists"
                    type="text"
                    placeholder="Filter..."
                    value={searchFilterValue() ?? ''}
                    onInput={debounce(async (e) => {
                        await loadArtists({
                            filters: {
                                search: e.target.value ?? undefined,
                            },
                        });
                        document.querySelector('main')?.scroll({
                            top: 0,
                            behavior: 'instant',
                        });
                    }, 200)}
                />
            </header>
            <div class="artists-page">
                <Show when={artists()}>
                    {(artists) => (
                        <div
                            class={`artists-container${
                                loading() ? ' loading' : ' loaded'
                            }`}
                        >
                            <p class="artists-header-artist-count">
                                Showing {artists()?.length} artist
                                {artists()?.length === 1 ? '' : 's'}
                            </p>
                            <div class="artists">
                                <For each={artists()}>
                                    {(artist) => (
                                        <Artist
                                            artist={artist}
                                            size={200}
                                            title={true}
                                        />
                                    )}
                                </For>
                            </div>
                        </div>
                    )}
                </Show>
            </div>
        </>
    );
}
