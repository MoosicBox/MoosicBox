import './albums-page.css';
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
import Album from '~/components/Album';
import { api, Api, once } from '~/services/api';
import { currentAlbumSearch, setCurrentAlbumSearch } from '~/services/app';
import { QueryParams } from '~/services/util';

let historyListener: () => void;

export default function albums() {
    let albumsHeaderContainerRef: HTMLDivElement;
    let backToTopRef: HTMLDivElement;

    const [loading, setLoading] = createSignal(false);
    const [albums, setAlbums] = createSignal<Api.LibraryAlbum[]>();
    const [searchFilterValue, setSearchFilterValue] = createSignal<string>();
    const [currentAlbumSort, setCurrentAlbumSort] =
        createSignal<Api.AlbumSort>('Artist');
    const [currentAlbumSources, setCurrentAlbumSources] = createSignal<
        Api.AlbumSource[]
    >([]);
    const [showAlbumControls, setShowAlbumControls] = createSignal(false);

    const searchParams = new QueryParams(
        isServer ? {} : window.location.search,
    );

    createComputed(() => {
        if (searchParams.has('sort')) {
            setCurrentAlbumSort(searchParams.get('sort') as Api.AlbumSort);
        }
        if (searchParams.has('sources')) {
            setCurrentAlbumSources(getAlbumSources() ?? []);
        }
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
            albumsHeaderContainerRef.getBoundingClientRect().bottom
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

    function setSearchParam(name: string, value: string) {
        searchParams.set(name, value);
        const newRelativePathQuery =
            searchParams.size === 0
                ? window.location.pathname
                : `${window.location.pathname}?${searchParams}`;
        history.pushState(null, '', newRelativePathQuery);

        switch (name) {
            case 'sort':
                setCurrentAlbumSort(value as Api.AlbumSort);
                break;
            case 'sources':
                setCurrentAlbumSources(value.split(',') as Api.AlbumSource[]);
                break;
        }
    }

    function removeSearchParam(name: string) {
        searchParams.delete(name);
        const newRelativePathQuery =
            searchParams.size === 0
                ? window.location.pathname
                : `${window.location.pathname}?${searchParams}`;
        history.pushState(null, '', newRelativePathQuery);
    }

    function getAlbumSources(): Api.AlbumSource[] | undefined {
        return searchParams.get('sources')?.split(',') as
            | Api.AlbumSource[]
            | undefined;
    }

    function getAlbumSort(): Api.AlbumSort | undefined {
        return searchParams.get('sort') as Api.AlbumSort | undefined;
    }

    function getSearchFilter(): string | undefined {
        return searchParams.get('search') as string | undefined;
    }

    function setAlbumSources(sources: Api.AlbumSource[]) {
        setSearchParam('sources', sources.join(','));
    }

    function setAlbumSort(sort: Api.AlbumSort) {
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

    async function toggleAlbumSource(
        source: Api.AlbumSource,
        enabled?: boolean | undefined,
    ) {
        const current = currentAlbumSources();

        if (typeof enabled === 'undefined') {
            enabled = !current.includes(source);
        }

        const sources = current.filter((x) => {
            return enabled || x !== source;
        });

        if (enabled && !sources.includes(source)) {
            sources.push(source);
        }

        await loadAlbums({ sources });
    }

    async function loadAlbums(
        request: Api.AlbumsRequest | undefined = undefined,
    ) {
        const query = searchParams.toString();
        if (!albums()) {
            const current = currentAlbumSearch();
            if (current && current.query === query) {
                setAlbums(current.results);
                return;
            }
        }
        if (request?.sources) setAlbumSources(request.sources);
        if (request?.sort) setAlbumSort(request.sort);
        if (typeof request?.filters?.search === 'string')
            setSearchFilter(request.filters.search);

        setLoading(true);
        await once('albums', (signal) =>
            api.getAllAlbums(
                {
                    sources: getAlbumSources(),
                    sort: currentAlbumSort(),
                    filters: {
                        search: getSearchFilter(),
                    },
                },
                (_new, albums) => {
                    setAlbums(albums);
                    setLoading(false);
                },
                signal,
            ),
        );

        const results = albums();

        if (results) {
            setCurrentAlbumSearch({ query, results });
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
                loadAlbums();
            }
        };

        window.addEventListener('popstate', historyListener);
    }

    onCleanup(() => {
        if (historyListener) {
            window.removeEventListener('popstate', historyListener);
        }
    });

    onMount(async () => {
        if (isServer) return;
        setSearchFilterValue(getSearchFilter() ?? '');
        await loadAlbums();
    });

    const handleAlbumSortClick = (_event: MouseEvent) => {
        if (!showAlbumControls()) return;
        setShowAlbumControls(false);
    };

    onMount(() => {
        if (isServer) return;
        window.addEventListener('click', handleAlbumSortClick);
    });

    onCleanup(() => {
        if (isServer) return;
        window.removeEventListener('click', handleAlbumSortClick);
    });

    return (
        <>
            <div class="albums-back-to-top-container main-content-back-to-top">
                <div
                    onClick={() =>
                        document.querySelector('main')?.scroll({
                            top: 0,
                            behavior: 'smooth',
                        })
                    }
                    class="albums-back-to-top"
                    ref={backToTopRef!}
                >
                    <div class="albums-back-to-top-content">
                        <img
                            class="albums-back-to-top-chevron"
                            src="/img/chevron-up-white.svg"
                        />
                        Back to top
                        <img
                            class="albums-back-to-top-chevron"
                            src="/img/chevron-up-white.svg"
                        />
                    </div>
                </div>
            </div>
            <div
                class="albums-header-container"
                ref={albumsHeaderContainerRef!}
            >
                <div class="albums-header-backdrop"></div>
                <div class="albums-header-text-container">
                    <h1 class="albums-header-text">
                        Albums{' '}
                        <img
                            class="albums-header-sort-icon"
                            src="/img/more-options-white.svg"
                            onClick={(event) => {
                                setShowAlbumControls(!showAlbumControls());
                                event.stopPropagation();
                            }}
                        />
                    </h1>
                    {showAlbumControls() && (
                        <div class="albums-controls">
                            <div class="albums-sort-controls">
                                <div
                                    onClick={() =>
                                        loadAlbums({ sort: 'Artist' })
                                    }
                                >
                                    Album Artist{' '}
                                    {currentAlbumSort() === 'Artist' && (
                                        <img
                                            class="sort-chevron-icon"
                                            src="/img/chevron-up-white.svg"
                                        />
                                    )}
                                    {currentAlbumSort() === 'Artist-Desc' && (
                                        <img
                                            class="sort-chevron-icon"
                                            src="/img/chevron-down-white.svg"
                                        />
                                    )}
                                </div>
                                <div
                                    onClick={() => loadAlbums({ sort: 'Name' })}
                                >
                                    Album Name
                                    {currentAlbumSort() === 'Name' && (
                                        <img
                                            class="sort-chevron-icon"
                                            src="/img/chevron-up-white.svg"
                                        />
                                    )}
                                    {currentAlbumSort() === 'Name-Desc' && (
                                        <img
                                            class="sort-chevron-icon"
                                            src="/img/chevron-down-white.svg"
                                        />
                                    )}
                                </div>
                                <div
                                    onClick={() =>
                                        loadAlbums({
                                            sort:
                                                getAlbumSort() ===
                                                'Release-Date-Desc'
                                                    ? 'Release-Date'
                                                    : 'Release-Date-Desc',
                                        })
                                    }
                                >
                                    Album Release Date
                                    {currentAlbumSort() === 'Release-Date' && (
                                        <img
                                            class="sort-chevron-icon"
                                            src="/img/chevron-up-white.svg"
                                        />
                                    )}
                                    {currentAlbumSort() ===
                                        'Release-Date-Desc' && (
                                        <img
                                            class="sort-chevron-icon"
                                            src="/img/chevron-down-white.svg"
                                        />
                                    )}
                                </div>
                                <div
                                    onClick={() =>
                                        loadAlbums({
                                            sort:
                                                getAlbumSort() ===
                                                'Date-Added-Desc'
                                                    ? 'Date-Added'
                                                    : 'Date-Added-Desc',
                                        })
                                    }
                                >
                                    Album Date Added
                                    {currentAlbumSort() === 'Date-Added' && (
                                        <img
                                            class="sort-chevron-icon"
                                            src="/img/chevron-up-white.svg"
                                        />
                                    )}
                                    {currentAlbumSort() ===
                                        'Date-Added-Desc' && (
                                        <img
                                            class="sort-chevron-icon"
                                            src="/img/chevron-down-white.svg"
                                        />
                                    )}
                                </div>
                            </div>
                            <div class="albums-filter-controls">
                                <div onClick={() => toggleAlbumSource('Local')}>
                                    Local{' '}
                                    <input
                                        type="checkbox"
                                        checked={currentAlbumSources().includes(
                                            'Local',
                                        )}
                                    />
                                </div>
                                <div onClick={() => toggleAlbumSource('Tidal')}>
                                    Tidal{' '}
                                    <input
                                        type="checkbox"
                                        checked={currentAlbumSources().includes(
                                            'Tidal',
                                        )}
                                    />
                                </div>
                                <div onClick={() => toggleAlbumSource('Qobuz')}>
                                    Qobuz{' '}
                                    <input
                                        type="checkbox"
                                        checked={currentAlbumSources().includes(
                                            'Qobuz',
                                        )}
                                    />
                                </div>
                            </div>
                        </div>
                    )}
                </div>
                <input
                    class="filter-albums"
                    type="text"
                    placeholder="Filter..."
                    value={searchFilterValue() ?? ''}
                    onInput={debounce(async (e) => {
                        await loadAlbums({
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
            </div>
            <div
                class={`albums-container${loading() ? ' loading' : ' loaded'}`}
            >
                <Show when={albums()}>
                    {(albums) => (
                        <>
                            <p class="albums-header-album-count">
                                Showing {albums()?.length} album
                                {albums()?.length === 1 ? '' : 's'}
                            </p>
                            <div class="albums">
                                <For each={albums()}>
                                    {(album) => (
                                        <Album
                                            album={album}
                                            controls={true}
                                            artist={true}
                                            title={true}
                                            year={true}
                                            versionQualities={true}
                                        />
                                    )}
                                </For>
                            </div>
                        </>
                    )}
                </Show>
            </div>
        </>
    );
}
