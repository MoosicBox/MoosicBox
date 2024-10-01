import './album.css';
import {
    type Album,
    type AlbumType,
    Api,
    type Track,
    api,
} from '~/services/api';
import { addAlbumToQueue, playAlbum } from '~/services/player';
import { createComputed, createSignal } from 'solid-js';
import { displayAlbumVersionQualities } from '~/services/formatting';
import { artistRoute } from '../Artist/Artist';

function albumControls(album: Album | Track) {
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
    const albumType = props.album.type;

    switch (albumType) {
        case 'LIBRARY':
            return props.album.title;
        case 'TIDAL': {
            let title = props.album.title;

            if (props.album.mediaMetadataTags?.includes('DOLBY_ATMOS')) {
                title += ' (Dolby Atmos)';
            }

            return title;
        }
        case 'QOBUZ':
            return props.album.title;
        case 'YT':
            return props.album.title;
        default:
            albumType satisfies never;
            throw new Error(`Invalid albumType: ${albumType}`);
    }
}

function isExplicit(props: AlbumProps): boolean {
    const albumType = props.album.type;

    switch (albumType) {
        case 'LIBRARY':
            return false;
        case 'TIDAL':
            return props.album.explicit;
        case 'QOBUZ':
            return props.album.parentalWarning;
        case 'YT':
            return false;
        default:
            albumType satisfies never;
            throw new Error(`Invalid albumType: ${albumType}`);
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
        | Album
        | Track
        | { id: number | string; type: AlbumType }
        | { albumId: number | string; type: AlbumType },
): string {
    const albumType = album.type;

    switch (albumType) {
        case 'LIBRARY':
            if ('albumId' in album) {
                return `/albums?albumId=${
                    (album as { albumId: number | string }).albumId
                }`;
            } else if ('id' in album) {
                return `/albums?albumId=${
                    (album as { id: number | string }).id
                }`;
            } else {
                throw new Error(`Invalid album: ${album}`);
            }
        case 'TIDAL':
            if ('number' in album) {
                return `/albums?tidalAlbumId=${
                    (album as Api.TidalTrack).albumId
                }`;
            } else {
                return `/albums?tidalAlbumId=${
                    (album as { id: number | string }).id
                }`;
            }
        case 'QOBUZ':
            if ('number' in album) {
                return `/albums?qobuzAlbumId=${
                    (album as Api.QobuzTrack).albumId
                }`;
            } else {
                return `/albums?qobuzAlbumId=${
                    (album as { id: number | string }).id
                }`;
            }
        case 'YT':
            if ('number' in album) {
                return `/albums?ytAlbumId=${(album as Api.YtTrack).albumId}`;
            } else {
                return `/albums?ytAlbumId=${
                    (album as { id: number | string }).id
                }`;
            }
        default:
            albumType satisfies never;
            throw new Error(`Invalid albumType: ${albumType}`);
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
    album: Album | Track;
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
