import './album.css';
import { Api, api, ApiSource } from '~/services/api';
import { addAlbumToQueue, playAlbum } from '~/services/player';
import { createComputed, createSignal } from 'solid-js';
import { displayAlbumVersionQualities } from '~/services/formatting';
import { artistRoute } from '../Artist/Artist';

function albumControls(album: Api.Album | Api.Track) {
    return (
        <div class="album-controls">
            <button
                class="media-button play-button button"
                onClick={(e) => {
                    e.stopPropagation();
                    e.preventDefault();
                    playAlbum(album);
                    return false;
                }}
            >
                <img src="/img/play-button.svg" alt="Play" />
            </button>
            <button
                class="media-button options-button button"
                onClick={(e) => {
                    e.stopPropagation();
                    e.preventDefault();
                    addAlbumToQueue(album);
                    return false;
                }}
            >
                <img src="/img/more-options.svg" alt="Play" />
            </button>
        </div>
    );
}

function getAlbumTitleDisplay(props: AlbumProps): string {
    if ('album' in props.album) {
        return props.album.album;
    } else {
        return props.album.title;
    }
}

function isExplicit(props: AlbumProps): boolean {
    const apiSource = props.album.apiSource;

    switch (apiSource) {
        case 'LIBRARY':
            return false;
        case 'TIDAL':
            return false;
        case 'QOBUZ':
            return false;
        case 'YT':
            return false;
        default:
            apiSource satisfies never;
            throw new Error(
                `isExplicit: Invalid apiSource: ${JSON.stringify(apiSource)}`,
            );
    }
}

const wordsCache: { [str: string]: string[] } = {};

function getWords(str: string) {
    const words = wordsCache[str] ?? str.split(' ');

    wordsCache[str] = words;

    return words;
}

function allButLastWord(str: string): string {
    const words = getWords(str);
    return words.slice(0, words.length - 1).join(' ');
}

function lastWord(str: string): string {
    const words = getWords(str);
    return words[words.length - 1]!;
}

function albumDetails(props: AlbumProps) {
    return (
        <div class="album-details">
            {props.title && (
                <div class="album-title">
                    {props.route ? (
                        <a
                            href={albumRoute(props.album)}
                            class="album-title-text"
                            title={`${props.album.title}${isExplicit(props) ? ' (Explicit)' : ''}`}
                        >
                            {allButLastWord(getAlbumTitleDisplay(props))}
                            {lastWord(getAlbumTitleDisplay(props)) ? (
                                <>
                                    {' '}
                                    <span class="album-details-explicit-wordwrap">
                                        {lastWord(getAlbumTitleDisplay(props))}
                                        {isExplicit(props) && (
                                            <img
                                                class="album-details-explicit"
                                                src="/img/explicit.svg"
                                                alt="Explicit"
                                            />
                                        )}
                                    </span>
                                </>
                            ) : (
                                isExplicit(props) && (
                                    <img
                                        class="album-details-explicit"
                                        src="/img/explicit.svg"
                                        alt="Explicit"
                                    />
                                )
                            )}
                        </a>
                    ) : (
                        <span
                            class="album-title-text"
                            title={`${props.album.title}${isExplicit(props) ? ' (Explicit)' : ''}`}
                        >
                            {props.album.title}
                        </span>
                    )}
                </div>
            )}
            {props.artist && (
                <div class="album-artist">
                    <a
                        href={artistRoute(props.album)}
                        class="album-artist-text"
                    >
                        {props.album.artist}
                    </a>
                </div>
            )}
            {props.year && 'dateReleased' in props.album && (
                <div class="album-year">
                    <span class="album-year-text">
                        {props.album.dateReleased?.substring(0, 4)}
                    </span>
                </div>
            )}
            {'versions' in props.album && props.versionQualities && (
                <div class="album-version-qualities">
                    <span class="album-version-qualities-text">
                        {props.album.versions.length > 0 &&
                            displayAlbumVersionQualities(props.album.versions)}
                    </span>
                </div>
            )}
        </div>
    );
}

export function albumRoute(
    album:
        | Api.Album
        | Api.Track
        | {
              albumId: number | string;
              apiSource: ApiSource | Api.DownloadApiSource;
          },
): string {
    const apiSource = album.apiSource;

    switch (apiSource) {
        case 'LIBRARY':
            return `/albums?albumId=${album.albumId}`;
        case 'TIDAL':
            return `/albums?tidalAlbumId=${album.albumId}`;
        case 'QOBUZ':
            return `/albums?qobuzAlbumId=${album.albumId}`;
        case 'YT':
            return `/albums?ytAlbumId=${album.albumId}`;
        default:
            // FIXME: This is a hack to get around the fact that the
            // MOOSIC_BOX api source doesn't have a linkable route
            if (typeof apiSource === 'object' && 'MOOSIC_BOX' in apiSource) {
                return '/';
            }
            if (typeof apiSource === 'object' && 'source' in apiSource) {
                switch (apiSource.source) {
                    case 'TIDAL':
                        return `/albums?tidalAlbumId=${album.albumId}`;
                    case 'QOBUZ':
                        return `/albums?qobuzAlbumId=${album.albumId}`;
                    case 'YT':
                        return `/albums?ytAlbumId=${album.albumId}`;
                }
            }

            apiSource satisfies never;
            throw new Error(
                `albumRoute: Invalid apiSource: ${JSON.stringify(apiSource)}`,
            );
    }
}

function albumImage(props: AlbumProps, blur: boolean) {
    return (
        <img
            class="album-icon"
            style={{
                width: `${props.size}px`,
                height: `${props.size}px`,
                'image-rendering': blur ? 'pixelated' : undefined,
                cursor: props.onClick ? `pointer` : undefined,
            }}
            src={api.getAlbumArtwork(
                props.album,
                blur ? 16 : props.imageRequestSize,
                blur ? 16 : props.imageRequestSize,
            )}
            alt={`${props.album.title} by ${props.album.artist}`}
            title={`${props.album.title} by ${props.album.artist}`}
            loading="lazy"
            onClick={props.onClick ?? (() => {})}
        />
    );
}

type PartialBy<T, K extends keyof T> = Omit<T, K> & Partial<Pick<T, K>>;

type AlbumProps = {
    album: Api.Album | Api.Track;
    controls?: boolean;
    size: number;
    imageRequestSize: number;
    artist: boolean;
    year: boolean;
    title: boolean;
    versionQualities: boolean;
    blur: boolean;
    route: boolean;
    onClick?: (e: MouseEvent) => void;
};

export default function album(
    props: PartialBy<
        AlbumProps,
        | 'size'
        | 'imageRequestSize'
        | 'artist'
        | 'title'
        | 'blur'
        | 'route'
        | 'year'
        | 'versionQualities'
    >,
) {
    props.size = props.size ?? 200;
    props.imageRequestSize =
        props.imageRequestSize ??
        Math.ceil(Math.round(Math.max(200, props.size) * 1.33) / 20) * 20;
    props.artist = props.artist ?? false;
    props.title = props.title ?? false;
    props.route = props.route ?? true;
    props.year = props.year ?? false;
    props.versionQualities = props.versionQualities ?? false;

    const fullProps = props as AlbumProps;

    const [blur, setBlur] = createSignal(false);

    createComputed(() => {
        setBlur(
            typeof fullProps.blur === 'boolean'
                ? fullProps.blur
                : 'blur' in fullProps.album && fullProps.album.blur,
        );
    });

    return (
        <div class="album">
            <div
                class="album-icon-container"
                style={{
                    width: `${fullProps.size}px`,
                    height: `${fullProps.size}px`,
                }}
            >
                {fullProps.route ? (
                    <a href={albumRoute(fullProps.album)}>
                        {albumImage(fullProps as AlbumProps, blur())}
                        {fullProps.controls && albumControls(fullProps.album)}
                    </a>
                ) : (
                    <>
                        {albumImage(fullProps as AlbumProps, blur())}
                        {fullProps.controls && albumControls(fullProps.album)}
                    </>
                )}
            </div>
            {(fullProps.artist || fullProps.title) && albumDetails(fullProps)}
        </div>
    );
}
