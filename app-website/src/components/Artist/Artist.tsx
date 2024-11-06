import './artist.css';
import { Api, api } from '~/services/api';
import type { ApiSource, Track } from '~/services/api';
import { createComputed, createSignal } from 'solid-js';

export function artistRoute(
    artist:
        | Api.Artist
        | Api.Album
        | Track
        | { id: string | number; apiSource: ApiSource }
        | { artistId: string | number; apiSource: ApiSource },
): string {
    const apiSource = artist.apiSource;

    switch (apiSource) {
        case 'LIBRARY':
            return `/artists?artistId=${
                (artist as { artistId: number }).artistId
            }`;
        case 'TIDAL':
            return `/artists?tidalArtistId=${
                (artist as { artistId: number }).artistId
            }`;
        case 'QOBUZ':
            return `/artists?qobuzArtistId=${
                (artist as { artistId: number }).artistId
            }`;
        case 'YT':
            return `/artists?ytArtistId=${
                (artist as { artistId: number }).artistId
            }`;
        default:
            apiSource satisfies never;
            throw new Error(`Invalid apiSource: ${apiSource}`);
    }
}

function artistDetails(artist: Api.Artist, showTitle = true) {
    return (
        <div class="artist-details">
            {showTitle && (
                <div class="artist-title">
                    <a class="artist-title-link" href={artistRoute(artist)}>
                        <span class="artist-title-text">{artist.title}</span>
                    </a>
                </div>
            )}
        </div>
    );
}

function artistImage(props: ArtistProps, blur: boolean) {
    return (
        <img
            class="artist-icon"
            style={{
                width: `${props.size}px`,
                height: `${props.size}px`,
                filter: blur ? `blur(${props.size / 20}px)` : undefined,
                cursor: props.onClick ? `pointer` : undefined,
            }}
            src={api.getArtistCover(
                props.artist,
                props.imageRequestSize,
                props.imageRequestSize,
            )}
            alt={`${props.artist.title}`}
            title={`${props.artist.title}`}
            loading="lazy"
            onClick={props.onClick ?? (() => {})}
        />
    );
}

type PartialBy<T, K extends keyof T> = Omit<T, K> & Partial<Pick<T, K>>;

type ArtistProps = {
    artist: Api.Artist;
    size: number;
    imageRequestSize: number;
    title: boolean;
    blur: boolean;
    route: boolean;
    onClick?: (e: MouseEvent) => void;
};

export default function artist(
    props: PartialBy<
        ArtistProps,
        'size' | 'imageRequestSize' | 'title' | 'blur' | 'route'
    >,
) {
    props.size = props.size ?? 200;
    props.imageRequestSize =
        props.imageRequestSize ??
        Math.ceil(Math.round(Math.max(200, props.size) * 1.33) / 20) * 20;
    props.title = props.title ?? false;
    props.route = props.route ?? true;

    const [blur, setBlur] = createSignal(false);

    createComputed(() => {
        setBlur(typeof props.blur === 'boolean' ? props.blur : false);
    });

    return (
        <div class="artist">
            <div
                class="artist-icon-container"
                style={{ width: `${props.size}px`, height: `${props.size}px` }}
            >
                {props.route ? (
                    <a href={artistRoute(props.artist)}>
                        {artistImage(props as ArtistProps, blur())}
                    </a>
                ) : (
                    artistImage(props as ArtistProps, blur())
                )}
            </div>
            {(props.artist || props.title) &&
                artistDetails(props.artist, props.title)}
        </div>
    );
}
