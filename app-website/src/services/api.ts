import { createSignal } from 'solid-js';
import type { Setter } from 'solid-js';
import {
    QueryParams,
    clientAtom,
    createListener,
    objToStr,
    throwExpression,
} from './util';
import { makePersisted } from '@solid-primitives/storage';

export type ScanOrigin = 'LOCAL' | 'TIDAL' | 'QOBUZ' | 'YT';
export type ApiSource = 'LIBRARY' | 'TIDAL' | 'QOBUZ' | 'YT';
export type Id = string | number;

export function trackId(
    track: Api.Track | number | undefined,
): string | number | undefined {
    if (!track) return undefined;
    if (typeof track === 'number' || typeof track === 'string') return track;
    return 'trackId' in track ? track.trackId : undefined;
}

export namespace Api {
    const onSignatureTokenUpdatedListeners =
        createListener<(url: string) => void>();
    export const onSignatureTokenUpdated = onSignatureTokenUpdatedListeners.on;
    export const offSignatureTokenUpdated =
        onSignatureTokenUpdatedListeners.off;
    const [_signatureToken, _setSignatureToken] = makePersisted(
        createSignal<string | undefined>(),
        {
            name: 'api.v2.signatureToken',
        },
    );
    export function signatureToken(): ReturnType<typeof _signatureToken> {
        return _signatureToken();
    }
    export function setSignatureToken(url: string): void {
        if (url === _signatureToken()) {
            return;
        }
        _setSignatureToken(url);

        onSignatureTokenUpdatedListeners.trigger(url);
    }

    export type GlobalSearchResultType = 'ARTIST' | 'ALBUM' | 'TRACK';
    export type GlobalSearchResult = (
        | GlobalArtistSearchResult
        | GlobalAlbumSearchResult
        | GlobalTrackSearchResult
    ) & { type: GlobalSearchResultType };

    export interface GlobalArtistSearchResult {
        type: 'ARTIST';
        artistId: string | number;
        title: string;
        containsCover: boolean;
        blur: boolean;
    }

    export interface GlobalAlbumSearchResult {
        type: 'ALBUM';
        artistId: string | number;
        artist: string;
        albumId: string | number;
        title: string;
        containsCover: boolean;
        blur: boolean;
        dateReleased: string;
        dateAdded: string;
        versions: AlbumVersionQuality[];
    }

    export interface GlobalTrackSearchResult {
        type: 'TRACK';
        artistId: string | number;
        artist: string;
        albumId: string | number;
        album: string;
        trackId: string | number;
        title: string;
        containsCover: boolean;
        blur: boolean;
        dateReleased: string;
        dateAdded: string;
        format: PlaybackQuality['format'];
        bitDepth: number;
        audioBitrate: number;
        overallBitrate: number;
        sampleRate: number;
        channels: number;
        source: TrackSource;
    }

    export enum PlayerType {
        HOWLER = 'HOWLER',
    }

    export interface Player {
        playerId: number;
        audioOutputId: string;
        name: string;
    }

    export interface Connection {
        connectionId: string;
        name: string;
        alive: boolean;
        players: Player[];
    }

    export interface Artist {
        artistId: number;
        title: string;
        containsCover: boolean;
        apiSource: ApiSource;
        apiSources: ApiSourceMapping[];
    }

    export enum TrackSource {
        LOCAL = 'LOCAL',
        TIDAL = 'TIDAL',
        QOBUZ = 'QOBUZ',
        YT = 'YT',
    }

    export interface AlbumVersionQuality {
        format: PlaybackQuality['format'] | null;
        bitDepth: number | null;
        sampleRate: number | null;
        channels: number | null;
        source: TrackSource;
    }

    export interface ApiSourceMapping {
        source: ApiSource;
        id: string | number;
    }

    export type AlbumType =
        | 'LP'
        | 'LIVE'
        | 'COMPILATIONS'
        | 'EPS_AND_SINGLES'
        | 'OTHER'
        | 'DOWNLOAD';

    export interface Album {
        albumId: string | number;
        title: string;
        artist: string;
        artistId: string | number;
        containsCover: boolean;
        blur: boolean;
        dateReleased: string;
        dateAdded: string;
        albumType: AlbumType;
        albumSource: TrackSource;
        apiSource: ApiSource;
        versions: AlbumVersionQuality[];
        albumSources: ApiSourceMapping[];
        artistSources: ApiSourceMapping[];
    }

    export interface LibraryAlbum {
        albumId: number;
        title: string;
        artist: string;
        artistId: number;
        containsCover: boolean;
        blur: boolean;
        dateReleased: string;
        dateAdded: string;
        versions: AlbumVersionQuality[];
        tidalId?: number;
        qobuzId?: string;
        ytId?: string;
        tidalArtistId?: number;
        qobuzArtistId?: number;
        ytArtistId?: string;
        type: 'LIBRARY';
    }

    export interface Track {
        trackId: number;
        number: number;
        title: string;
        duration: number;
        album: string;
        albumId: number;
        dateReleased: string;
        artist: string;
        artistId: number;
        containsCover: boolean;
        blur: boolean;
        bytes: number;
        format: PlaybackQuality['format'];
        bitDepth: number;
        audioBitrate: number;
        overallBitrate: number;
        sampleRate: number;
        channels: number;
        apiSource: ApiSource;
    }

    export interface AlbumVersion {
        tracks: Api.Track[];
        format: PlaybackQuality['format'] | null;
        bitDepth: number | null;
        audioBitrate: number | null;
        overallBitrate: number | null;
        sampleRate: number | null;
        channels: number | null;
        source: TrackSource;
    }

    export const AudioFormat = {
        AAC: 'AAC',
        FLAC: 'FLAC',
        MP3: 'MP3',
        OPUS: 'OPUS',
        SOURCE: 'SOURCE',
    } as const;

    export interface PlaybackQuality {
        format: keyof typeof AudioFormat;
    }

    export interface UpdatePlaybackSession {
        sessionId: number;
        profile: string;
        name?: string;
        active?: boolean;
        playing?: boolean;
        position?: number;
        seek?: number;
        volume?: number;
        playbackTarget?: PlaybackTarget;
        playlist?: UpdatePlaybackSessionPlaylist;
    }

    export interface UpdatePlaybackSessionPlaylist {
        sessionPlaylistId: number;
        tracks: Api.Track[];
    }

    export interface AudioZone {
        id: number;
        name: string;
        players: Api.Player[];
    }

    export interface UpdateAudioZone {
        id: number;
        name?: string;
        players?: number[];
    }

    export interface AudioZonePlaybackTarget {
        type: 'AUDIO_ZONE';
        audioZoneId: number;
    }

    export interface ConnectionOutputPlaybackTarget {
        type: 'CONNECTION_OUTPUT';
        connectionId: string;
        outputId: string;
    }

    export type PlaybackTarget =
        | AudioZonePlaybackTarget
        | ConnectionOutputPlaybackTarget;

    export interface PlaybackSession {
        sessionId: number;
        profile: string;
        playbackTarget: PlaybackTarget;
        name: string;
        active: boolean;
        playing: boolean;
        position?: number;
        seek?: number;
        volume?: number;
        playlist: PlaybackSessionPlaylist;
        quality?: PlaybackQuality;
    }

    export interface PlaybackSessionPlaylist {
        sessionPlaylistId: number;
        tracks: Api.Track[];
    }

    export type ArtistSort = 'Name' | 'Name-Desc';

    export type ArtistsRequest = {
        sources?: AlbumSource[] | undefined;
        sort?: ArtistSort | undefined;
        filters?: ArtistFilters | undefined;
    };

    export type ArtistFilters = {
        search?: string | undefined;
    };

    export type AlbumSource = 'Local' | 'Tidal' | 'Qobuz';
    export type AlbumSort =
        | 'Artist'
        | 'Artist-Desc'
        | 'Name'
        | 'Name-Desc'
        | 'Release-Date'
        | 'Release-Date-Desc'
        | 'Date-Added'
        | 'Date-Added-Desc';

    export type AlbumsRequest = {
        artistId?: string | number | undefined;
        tidalArtistId?: string | undefined;
        qobuzArtistId?: string | undefined;
        albumType?: AlbumType | undefined;
        source?: ApiSource | undefined;
        sources?: AlbumSource[] | undefined;
        sort?: AlbumSort | undefined;
        filters?: AlbumFilters | undefined;
        offset?: number | undefined;
        limit?: number | undefined;
    };

    export type AlbumFilters = {
        search?: string | undefined;
    };

    export function getPath(path: string): string {
        path = path[0] === '/' ? path.substring(1) : path;
        const containsQuery = path.includes('?');
        const params = [];
        const con = getConnection();

        const clientId = con.clientId;
        if (con.clientId) {
            params.push(`clientId=${encodeURIComponent(clientId)}`);
        }
        const signatureToken = Api.signatureToken();
        if (signatureToken) {
            params.push(`signature=${encodeURIComponent(signatureToken)}`);
        }
        if (con.staticToken) {
            params.push(`authorization=${encodeURIComponent(con.staticToken)}`);
        }

        const query = `${containsQuery ? '&' : '?'}${params.join('&')}`;

        return `${con.apiUrl}/${path}${query}`;
    }

    export type QobuzPagingResponse<T> = {
        items: T[];
        hasMore: boolean;
    };

    type BasePagingResponse<T> = {
        items: T[];
        count: number;
        offset: number;
        limit: number;
    };

    export type PagingResponseWithTotal<T> = BasePagingResponse<T> & {
        total: number;
        hasMore: boolean;
    };

    export type PagingResponseWithHasMore<T> = BasePagingResponse<T> & {
        hasMore: boolean;
    };

    export type PagingResponse<T> =
        | PagingResponseWithTotal<T>
        | PagingResponseWithHasMore<T>;

    export type DownloadTaskState =
        | 'PENDING'
        | 'PAUSED'
        | 'CANCELLED'
        | 'STARTED'
        | 'ERROR'
        | 'FINISHED';

    export type DownloadItemType = 'TRACK' | 'ALBUM_COVER' | 'ARTIST_COVER';
    export type TrackDownloadItem = {
        id: number;
        type: 'TRACK';
        artistId: string;
        albumId: string;
        trackId: string;
        title: string;
        source: DownloadApiSource;
        quality: TrackAudioQuality;
        containsCover: boolean;
    };
    export type AlbumCoverDownloadItem = {
        type: 'ALBUM_COVER';
        artistId: string;
        artist: string;
        albumId: string;
        title: string;
        source: DownloadApiSource;
        containsCover: boolean;
    };
    export type ArtistCoverDownloadItem = {
        type: 'ARTIST_COVER';
        artistId: string;
        albumId: string;
        title: string;
        source: DownloadApiSource;
        containsCover: boolean;
    };
    export type DownloadItem =
        | TrackDownloadItem
        | AlbumCoverDownloadItem
        | ArtistCoverDownloadItem;

    export const TrackAudioQuality = {
        Low: 'LOW', // MP3 320
        FlacLossless: 'FLAC_LOSSLESS', // FLAC 16 bit 44.1kHz
        FlacHiRes: 'FLAC_HI_RES', // FLAC 24 bit <= 96kHz
        FlacHighestRes: 'FLAC_HIGHEST_RES', // FLAC 24 bit > 96kHz <= 192kHz
    } as const;
    export type TrackAudioQuality =
        (typeof TrackAudioQuality)[keyof typeof TrackAudioQuality];

    export type DownloadApiSource = 'TIDAL' | 'QOBUZ' | 'YT';

    export interface DownloadTask {
        id: number;
        state: DownloadTaskState;
        item: DownloadItem;
        filePath: string;
        progress: number;
        bytes: number;
        totalBytes: number;
        speed?: number;
    }

    export interface DownloadLocation {
        id: number;
        path: string;
    }

    export interface ScanPaths {
        paths: string[];
    }

    export interface Profile {
        name: string;
    }
}

export interface Connection {
    id: number;
    name: string;
    apiUrl: string;
    profile?: string | undefined;
    profiles?: string[] | undefined;
    clientId: string;
    token: string;
    staticToken: string;
    players?: Api.Player[];
}

export async function setActiveProfile(profile: string) {
    const con = $connection();
    if (!con?.profiles?.some((x) => x === profile))
        throw new Error(`Invalid profile: ${profile}`);
    await setConnection(con.id, { profile });
}

export async function setActiveConnection(id: number) {
    const cons = connections.get();
    const existing = cons.find((x) => x.id === id);
    if (!existing) throw new Error(`Invalid connection id: ${id}`);
    const con = await setConnection(id, existing);
    connection.set(con);
    profile.set(con.profile);
}

export async function refreshConnectionProfiles(con: Connection) {
    const profiles = (await api.getProfiles(con)).map((x) => x.name);

    const update: Parameters<typeof setConnection>[1] = { profiles };

    if (con.profile && !profiles.includes(con.profile)) {
        if (profiles.length > 0) {
            update.profile = profiles[0];
        } else {
            update.profile = undefined;
        }
    } else if (profiles.length > 0) {
        update.profile = profiles[0];
    }

    await setConnectionInner(con.id, update, true);
}

export async function setConnection(
    id: number,
    values: Partial<Connection>,
): Promise<Connection> {
    return await setConnectionInner(id, values);
}

async function setConnectionInner(
    id: number,
    values: Partial<Connection>,
    recursed: boolean = false,
): Promise<Connection> {
    const con = connection.get();

    let updated: Connection;

    const existing = connections.get()?.find((x) => x.id === id);

    if (existing?.id === id) {
        updated = {
            id,
            name: values.name ?? existing?.name ?? '',
            apiUrl: values.apiUrl ?? existing?.apiUrl ?? '',
            profile: values.profile ?? existing?.profile,
            profiles: values.profiles ?? existing?.profiles,
            clientId: values.clientId ?? existing?.clientId ?? '',
            token: values.token ?? existing?.token ?? '',
            staticToken: values.staticToken ?? existing?.staticToken ?? '',
        };

        if (!con || con.id === id) {
            connection.set(updated);
            profile.set(updated.profile);
        }
    } else {
        updated = {
            id,
            name: values.name ?? '',
            apiUrl: values.apiUrl ?? '',
            profile: values.profile,
            profiles: values.profiles,
            clientId: values.clientId ?? '',
            token: values.token ?? '',
            staticToken: values.staticToken ?? '',
        };

        if (!con) {
            connection.set(updated);
            profile.set(updated.profile);
        }
    }

    const updatedConnections = connections.get();
    const existingI = updatedConnections.findIndex((x) => x.id === updated.id);
    if (existingI !== -1) {
        updatedConnections[existingI] = updated;
    } else {
        updatedConnections.push(updated);
    }
    connections.set([...updatedConnections]);

    if (!recursed && !updated.profile) {
        try {
            await refreshConnectionProfiles(updated);
        } catch (e) {
            console.error(
                `Failed to refreshConnectionProfiles: ${JSON.stringify(e)}`,
                e,
            );
        }
    }

    return updated;
}

export async function deleteConnection(con: Connection) {
    connections.set($connections().filter((x) => x.id !== con.id));

    if ($connection()?.id === con.id) {
        connection.set(null);

        if ($connections().length > 0) {
            await setConnection($connections()[0].id, {});
        }
    }
}

export const profile = clientAtom<string | undefined>(
    undefined,
    'app.profile.v1',
);

export const defaultDownloadLocation = clientAtom<number | undefined>(
    undefined,
    'app.defaultDownloadLocation.v1',
);

export const connections = clientAtom<Connection[]>([], 'api.v2.connections');
const $connections = () => connections.get();

export const connection = clientAtom<Connection | null>(
    $connections()[0] ?? null,
    'api.v2.connection',
);
const $connection = () => connection.get();

let connectionId = 1;

$connections()?.forEach((x) => {
    if (x.id >= connectionId) {
        connectionId = x.id + 1;
    }
});

export function getNewConnectionId(): number {
    return connectionId++;
}

export interface ApiType {
    getArtist(
        artistId: number,
        signal?: AbortSignal | null,
    ): Promise<Api.Artist>;
    getArtistCover(
        artist: Api.Artist | Api.Album | Api.Track | undefined,
        width: number,
        height: number,
    ): string;
    getArtistSourceCover(
        artist: Api.Artist | Api.Album | Api.Track | undefined,
    ): string;
    getAlbum(
        id: string | number,
        source?: ApiSource,
        signal?: AbortSignal | null,
    ): Promise<Api.Album>;
    getAlbums(
        request?: Api.AlbumsRequest | undefined,
        signal?: AbortSignal | null,
    ): Promise<Api.PagingResponseWithTotal<Api.Album>>;
    getAllAlbums(
        request: Api.AlbumsRequest | undefined,
        onAlbums?: (
            albums: Api.Album[],
            allAlbums: Api.Album[],
            index: number,
        ) => void,
        signal?: AbortSignal | null,
    ): Promise<Api.Album[]>;
    getAlbumArtwork(
        album: Api.Album | Api.Track | undefined,
        width: number,
        height: number,
        signal?: AbortSignal | null,
    ): string;
    getAlbumSourceArtwork(
        album: Api.Album | Api.Track | undefined,
        signal?: AbortSignal | null,
    ): string;
    getAlbumTracks(
        albumId: number,
        signal?: AbortSignal | null,
    ): Promise<Api.Track[]>;
    getAlbumVersions(
        albumId: string | number,
        source?: ApiSource,
        signal?: AbortSignal | null,
    ): Promise<Api.AlbumVersion[]>;
    getTracks(
        trackIds: number[],
        signal?: AbortSignal | null,
    ): Promise<Api.Track[]>;
    getArtists(
        request?: Api.ArtistsRequest | undefined,
        signal?: AbortSignal | null,
    ): Promise<Api.Artist[]>;
    fetchSignatureToken(
        signal?: AbortSignal | null,
    ): Promise<string | undefined>;
    refetchSignatureToken(signal?: AbortSignal | null): Promise<void>;
    validateSignatureToken(signal?: AbortSignal | null): Promise<void>;
    validateSignatureTokenAndClient(
        signature: string,
        signal?: AbortSignal | null,
    ): Promise<{ valid?: boolean; notFound?: boolean }>;
    magicToken(
        magicToken: string,
        signal?: AbortSignal | null,
    ): Promise<{ clientId: string; accessToken: string } | false>;
    globalSearch(
        query: string,
        offset?: number,
        limit?: number,
        signal?: AbortSignal | null,
    ): Promise<{ position: number; results: Api.GlobalSearchResult[] }>;
    searchExternalMusicApi(
        query: string,
        api: string,
        offset?: number,
        limit?: number,
        signal?: AbortSignal | null,
    ): Promise<{ position: number; results: Api.GlobalSearchResult[] }>;
    searchAll(
        query: string,
        offset?: number,
        limit?: number,
        onResults?: (
            results: Api.GlobalSearchResult[],
            allResults: Api.GlobalSearchResult[],
            source: ApiSource,
        ) => void,
        signal?: AbortSignal | null,
    ): Promise<Api.GlobalSearchResult[]>;
    getArtistFromTidalArtistId(
        tidalArtistId: string,
        signal?: AbortSignal | null,
    ): Promise<Api.Artist>;
    getArtistFromQobuzArtistId(
        qobuzArtistId: string,
        signal?: AbortSignal | null,
    ): Promise<Api.Artist>;
    getArtistFromTidalAlbumId(
        tidalAlbumId: string,
        signal?: AbortSignal | null,
    ): Promise<Api.Artist>;
    getTidalArtist(
        tidalArtistId: string,
        signal?: AbortSignal | null,
    ): Promise<Api.Artist>;
    getQobuzArtist(
        qobuzArtistId: string,
        signal?: AbortSignal | null,
    ): Promise<Api.Artist>;
    getAlbumFromTidalAlbumId(
        tidalAlbumId: string,
        signal?: AbortSignal | null,
    ): Promise<Api.Album>;
    getAlbumFromQobuzAlbumId(
        qobuzAlbumId: string,
        signal?: AbortSignal | null,
    ): Promise<Api.Album>;
    getLibraryAlbumsFromTidalArtistId(
        tidalArtistId: string,
        signal?: AbortSignal | null,
    ): Promise<Api.Album[]>;
    getLibraryAlbumsFromQobuzArtistId(
        qobuzArtistId: string,
        signal?: AbortSignal | null,
    ): Promise<Api.Album[]>;
    getTidalAlbum(
        tidalAlbumId: string,
        signal?: AbortSignal | null,
    ): Promise<Api.Album>;
    getQobuzAlbum(
        qobuzAlbumId: string,
        signal?: AbortSignal | null,
    ): Promise<Api.Album>;
    getTidalAlbumTracks(
        tidalAlbumId: string,
        signal?: AbortSignal | null,
    ): Promise<Api.PagingResponse<Api.Track>>;
    getQobuzAlbumTracks(
        qobuzAlbumId: string,
        signal?: AbortSignal | null,
    ): Promise<Api.PagingResponse<Api.Track>>;
    getYtAlbumTracks(
        ytAlbumId: string,
        signal?: AbortSignal | null,
    ): Promise<Api.PagingResponse<Api.Track>>;
    getTidalTrack(
        tidalTrackId: string,
        signal?: AbortSignal | null,
    ): Promise<Api.Track>;
    getTrackUrlForSource(
        trackId: Id,
        source: ApiSource,
        audioQuality: Api.TrackAudioQuality,
        signal?: AbortSignal | null,
    ): Promise<string>;
    addAlbumToLibrary(
        albumId: {
            tidalAlbumId?: string | number;
            qobuzAlbumId?: string | number;
        },
        signal?: AbortSignal | null,
    ): Promise<void>;
    removeAlbumFromLibrary(
        albumId: {
            tidalAlbumId?: string | number;
            qobuzAlbumId?: string | number;
        },
        signal?: AbortSignal | null,
    ): Promise<Api.Album>;
    refavoriteAlbum(
        albumId: {
            tidalAlbumId?: string | number;
            qobuzAlbumId?: string | number;
        },
        signal?: AbortSignal | null,
    ): Promise<Api.Album>;
    retryDownload(taskId: number, signal?: AbortSignal | null): Promise<void>;
    download(
        items: {
            trackId?: string | number;
            trackIds?: (string | number)[];
            albumId?: string | number;
            albumIds?: (string | number)[];
        },
        source: Api.DownloadApiSource,
        signal?: AbortSignal | null,
    ): Promise<void>;
    getDownloadTasks(
        signal?: AbortSignal | null,
    ): Promise<Api.PagingResponseWithTotal<Api.DownloadTask>>;
    getDownloadLocations(
        signal?: AbortSignal | null,
    ): Promise<Api.PagingResponseWithTotal<Api.DownloadLocation>>;
    addDownloadLocation(
        path: string,
        signal?: AbortSignal | null,
    ): Promise<Api.DownloadLocation>;
    getTrackVisualization(
        track: Api.Track | number,
        source: ApiSource,
        max: number,
        signal?: AbortSignal | null,
    ): Promise<number[]>;
    getAudioZones(
        signal?: AbortSignal | null,
    ): Promise<Api.PagingResponseWithTotal<Api.AudioZone>>;
    createAudioZone(
        name: string,
        signal?: AbortSignal | null,
    ): Promise<Api.AudioZone>;
    updateAudioZone(
        update: Api.UpdateAudioZone,
        signal?: AbortSignal | null,
    ): Promise<Api.AudioZone>;
    deleteAudioZone(id: number, signal?: AbortSignal | null): Promise<void>;
    runScan(origins: ScanOrigin[], signal?: AbortSignal | null): Promise<void>;
    startScan(
        origins: ScanOrigin[],
        signal?: AbortSignal | null,
    ): Promise<void>;
    enableScanOrigin(
        origin: ScanOrigin,
        signal?: AbortSignal | null,
    ): Promise<void>;
    addScanPath(path: string, signal?: AbortSignal | null): Promise<void>;
    getScanPaths(signal?: AbortSignal | null): Promise<Api.ScanPaths>;
    getProfiles(
        connection?: Connection,
        signal?: AbortSignal | null,
    ): Promise<Api.Profile[]>;
}

export function getConnection(): Connection {
    return $connection() ?? throwExpression('No connection selected');
}

async function getArtist(
    artistId: number,
    signal?: AbortSignal | null,
): Promise<Api.Artist> {
    const con = getConnection();

    const query = new QueryParams({
        artistId: `${artistId}`,
    });

    return await requestJson(`${con.apiUrl}/menu/artist?${query}`, {
        signal: signal ?? null,
    });
}

function getAlbumArtwork(
    album: Api.Album | Api.Track | undefined,
    width: number,
    height: number,
): string {
    if (!album) return '/img/album.svg';

    const apiSource = album.apiSource;
    const query = new QueryParams({
        source: apiSource,
        artistId: album.artistId?.toString(),
        moosicboxProfile: $connection()?.profile,
    });

    switch (apiSource) {
        case 'LIBRARY':
            if (album.containsCover) {
                return Api.getPath(
                    `files/albums/${album.albumId}/${width}x${height}?${query}`,
                );
            }
            break;

        case 'TIDAL':
            if (album.containsCover) {
                return Api.getPath(
                    `files/albums/${album.albumId}/${width}x${height}?${query}`,
                );
            }
            break;

        case 'QOBUZ':
            if (album.containsCover) {
                return Api.getPath(
                    `files/albums/${album.albumId}/${width}x${height}?${query}`,
                );
            }
            break;

        case 'YT':
            if (album.containsCover) {
                return Api.getPath(
                    `files/albums/${album.albumId}/${width}x${height}?${query}`,
                );
            }
            break;

        default:
            apiSource satisfies never;
    }

    return '/img/album.svg';
}

function getAlbumSourceArtwork(
    album: Api.Album | Api.Track | undefined,
): string {
    if (!album) return '/img/album.svg';

    const apiSource = album.apiSource;
    const query = new QueryParams({
        source: apiSource,
        artistId: album.artistId.toString(),
        moosicboxProfile: $connection()?.profile,
    });

    switch (apiSource) {
        case 'LIBRARY':
            if (album.containsCover) {
                return Api.getPath(
                    `files/albums/${album.albumId}/source?${query}`,
                );
            }
            break;

        case 'TIDAL':
            if (album.containsCover) {
                return Api.getPath(
                    `files/albums/${album.albumId}/source?${query}`,
                );
            }
            break;

        case 'QOBUZ':
            if (album.containsCover) {
                return Api.getPath(
                    `files/albums/${album.albumId}/source?${query}`,
                );
            }
            break;

        case 'YT':
            if (album.containsCover) {
                return Api.getPath(
                    `files/albums/${album.albumId}/source?${query}`,
                );
            }
            break;

        default:
            apiSource satisfies never;
    }

    return '/img/album.svg';
}

async function getAlbum(
    id: string | number,
    source: ApiSource = 'LIBRARY',
    signal?: AbortSignal | null,
): Promise<Api.Album> {
    const con = getConnection();

    const query = new QueryParams({
        albumId: `${id}`,
        source,
    });

    return await requestJson(`${con.apiUrl}/menu/album?${query}`, {
        signal: signal ?? null,
    });
}

async function getAlbums(
    albumsRequest: Api.AlbumsRequest | undefined = undefined,
    signal?: AbortSignal | null,
): Promise<Api.PagingResponseWithTotal<Api.Album>> {
    const con = getConnection();
    const query = new QueryParams({
        artistId: albumsRequest?.artistId?.toString(),
        tidalArtistId: albumsRequest?.tidalArtistId?.toString(),
        qobuzArtistId: albumsRequest?.qobuzArtistId?.toString(),
        source: albumsRequest?.source,
        albumType: albumsRequest?.albumType,
        offset: `${albumsRequest?.offset ?? 0}`,
        limit: `${albumsRequest?.limit ?? 100}`,
    });
    if (albumsRequest?.sources)
        query.set('sources', albumsRequest.sources.join(','));
    if (albumsRequest?.sort) query.set('sort', albumsRequest.sort);
    if (albumsRequest?.filters?.search)
        query.set('search', albumsRequest.filters.search);

    return await requestJson(`${con.apiUrl}/menu/albums?${query}`, {
        signal: signal ?? null,
    });
}

async function getAllAlbums(
    albumsRequest: Api.AlbumsRequest | undefined = undefined,
    onAlbums?: (
        albums: Api.Album[],
        allAlbums: Api.Album[],
        index: number,
    ) => void,
    signal?: AbortSignal | null,
): Promise<Api.Album[]> {
    let offset = albumsRequest?.offset ?? 0;
    let limit = albumsRequest?.limit ?? 100;

    albumsRequest = albumsRequest ?? { offset, limit };

    const page = await getAlbums(albumsRequest, signal);

    let items = page.items;

    onAlbums?.(page.items, items, 0);

    if (signal?.aborted || !page.hasMore) return items;

    offset = limit;
    limit = Math.min(Math.max(100, Math.ceil((page.total - limit) / 6)), 1000);

    const requests = [];

    do {
        requests.push({ ...albumsRequest, offset, limit });
        offset += limit;
    } while (offset < page.total);

    const output = [items, ...requests.map(() => [])];

    await Promise.all(
        requests.map(async (request, i) => {
            const page = await getAlbums(request, signal);

            output[i + 1] = page.items;

            items = output.flat();

            onAlbums?.(page.items, items, i + 1);

            return page;
        }),
    );

    return items;
}

function getArtistCover(
    artist: Api.Artist | Api.Album | Api.Track | undefined,
    width: number,
    height: number,
): string {
    if (!artist) return '/img/album.svg';

    const apiSource = artist.apiSource;
    const query = new QueryParams({
        source: apiSource,
        moosicboxProfile: $connection()?.profile,
    });

    switch (apiSource) {
        case 'LIBRARY':
            if (artist.containsCover) {
                return Api.getPath(
                    `files/artists/${artist.artistId}/${width}x${height}?${query}`,
                );
            }
            break;

        case 'TIDAL':
            if (artist.containsCover) {
                return Api.getPath(
                    `files/artists/${artist.artistId}/${width}x${height}?${query}`,
                );
            }
            break;

        case 'QOBUZ':
            if (artist.containsCover) {
                return Api.getPath(
                    `files/artists/${artist.artistId}/${width}x${height}?${query}`,
                );
            }
            break;

        case 'YT':
            if (artist.containsCover) {
                return Api.getPath(
                    `files/artists/${artist.artistId}/${width}x${height}?${query}`,
                );
            }
            break;

        default:
            apiSource satisfies never;
    }

    return '/img/album.svg';
}

function getArtistSourceCover(
    artist: Api.Artist | Api.Album | Api.Track | undefined,
): string {
    if (!artist) return '/img/album.svg';

    const apiSource = artist.apiSource;
    const query = new QueryParams({
        source: apiSource,
        moosicboxProfile: $connection()?.profile,
    });

    switch (apiSource) {
        case 'LIBRARY':
            if (artist.containsCover) {
                return Api.getPath(
                    `files/artists/${artist.artistId}/source?${query}`,
                );
            }
            break;

        case 'TIDAL':
            if (artist.containsCover) {
                return Api.getPath(
                    `files/artists/${artist.artistId}/source?${query}`,
                );
            }
            break;

        case 'QOBUZ':
            if (artist.containsCover) {
                return Api.getPath(
                    `files/artists/${artist.artistId}/source?${query}`,
                );
            }
            break;

        case 'YT':
            if (artist.containsCover) {
                return Api.getPath(
                    `files/artists/${artist.artistId}/source?${query}`,
                );
            }
            break;

        default:
            apiSource satisfies never;
    }

    return '/img/album.svg';
}

async function getAlbumTracks(
    albumId: number,
    signal?: AbortSignal | null,
): Promise<Api.Track[]> {
    const con = getConnection();
    return await requestJson(
        `${con.apiUrl}/menu/album/tracks?albumId=${albumId}`,
        {
            method: 'GET',
            signal: signal ?? null,
        },
    );
}

async function getAlbumVersions(
    albumId: string | number,
    source: ApiSource = 'LIBRARY',
    signal?: AbortSignal | null,
): Promise<Api.AlbumVersion[]> {
    const con = getConnection();
    const queryParams = new QueryParams({
        albumId: `${albumId}`,
        source,
    });

    return await requestJson(
        `${con.apiUrl}/menu/album/versions?${queryParams}`,
        {
            method: 'GET',
            signal: signal ?? null,
        },
    );
}

async function getTracks(
    trackIds: number[],
    signal?: AbortSignal | null,
): Promise<Api.Track[]> {
    const con = getConnection();
    return await requestJson(
        `${con.apiUrl}/menu/tracks?trackIds=${trackIds.join(',')}`,
        {
            method: 'GET',
            signal: signal ?? null,
        },
    );
}

async function getArtists(
    artistsRequest: Api.ArtistsRequest | undefined = undefined,
    signal?: AbortSignal | null,
): Promise<Api.Artist[]> {
    const con = getConnection();
    const query = new QueryParams();
    if (artistsRequest?.sources)
        query.set('sources', artistsRequest.sources.join(','));
    if (artistsRequest?.sort) query.set('sort', artistsRequest.sort);
    if (artistsRequest?.filters?.search)
        query.set('search', artistsRequest.filters.search);

    return await requestJson(`${con.apiUrl}/menu/artists?${query}`, {
        signal: signal ?? null,
    });
}

async function fetchSignatureToken(
    signal?: AbortSignal | null,
): Promise<string | undefined> {
    const con = getConnection();
    const { token } = await requestJson<{ token: string }>(
        `${con.apiUrl}/auth/signature-token`,
        {
            method: 'POST',
            signal: signal ?? null,
        },
    );

    return token;
}

const [nonTunnelApis, setNonTunnelApis] = makePersisted(
    createSignal<string[]>([]),
    {
        name: 'nonTunnelApis',
    },
);

async function validateSignatureTokenAndClient(
    signature: string,
    signal?: AbortSignal | null,
): Promise<{ valid?: boolean; notFound?: boolean }> {
    const con = getConnection();
    const apis = nonTunnelApis();

    if (apis.includes(con.apiUrl)) {
        return { notFound: true };
    }

    try {
        const { valid } = await requestJson<{ valid: boolean }>(
            `${con.apiUrl}/auth/validate-signature-token?signature=${signature}`,
            {
                method: 'POST',
                signal: signal ?? null,
            },
        );

        return { valid: !!valid };
    } catch (e) {
        if (e instanceof RequestError) {
            if (e.response.status === 404) {
                setNonTunnelApis([...apis, con.apiUrl]);
                return { notFound: true };
            }
        }

        return { valid: false };
    }
}

async function refetchSignatureToken(): Promise<void> {
    console.debug('Refetching signature token');
    const token = await api.fetchSignatureToken();

    if (token) {
        Api.setSignatureToken(token);
    } else {
        console.error('Failed to fetch signature token');
    }
}

async function validateSignatureToken(): Promise<void> {
    const con = getConnection();
    if (!con.token) return;

    const existing = Api.signatureToken();

    if (!existing) {
        await api.refetchSignatureToken();

        return;
    }

    const { valid, notFound } =
        await api.validateSignatureTokenAndClient(existing);

    if (notFound) {
        console.debug('Not hitting tunnel server');
        return;
    }

    if (!valid) {
        await api.refetchSignatureToken();
    }
}

async function magicToken(
    magicToken: string,
    signal?: AbortSignal | null,
): Promise<{ clientId: string; accessToken: string } | false> {
    const con = getConnection();
    try {
        return await requestJson(
            `${con.apiUrl}/auth/magic-token?magicToken=${magicToken}`,
            {
                signal: signal ?? null,
            },
        );
    } catch {
        return false;
    }
}

async function globalSearch(
    query: string,
    offset?: number,
    limit?: number,
    signal?: AbortSignal | null,
): Promise<{ position: number; results: Api.GlobalSearchResult[] }> {
    const con = getConnection();
    const queryParams = new QueryParams({
        query,
        offset: offset?.toString() ?? undefined,
        limit: limit?.toString() ?? undefined,
    });
    return await requestJson(
        `${con.apiUrl}/search/global-search?${queryParams.toString()}`,
        {
            signal: signal ?? null,
        },
    );
}

async function searchExternalMusicApi(
    query: string,
    api: string,
    offset?: number,
    limit?: number,
    signal?: AbortSignal | null,
): Promise<{ position: number; results: Api.GlobalSearchResult[] }> {
    const con = getConnection();
    const queryParams = new QueryParams({
        query,
        offset: offset?.toString() ?? undefined,
        limit: limit?.toString() ?? undefined,
    });
    return await requestJson(
        `${con.apiUrl}/${api}/search?${queryParams.toString()}`,
        {
            signal: signal ?? null,
        },
    );
}

async function searchAll(
    query: string,
    offset?: number,
    limit?: number,
    onResults?: (
        results: Api.GlobalSearchResult[],
        allResults: Api.GlobalSearchResult[],
        source: ApiSource,
    ) => void,
    signal?: AbortSignal | null,
): Promise<Api.GlobalSearchResult[]> {
    const allResults: Api.GlobalSearchResult[] = [];
    await Promise.all([
        (async () => {
            const results = (await globalSearch(query, offset, limit, signal))
                .results;
            allResults.push(...allResults);
            onResults?.(results, allResults, 'LIBRARY');
        })(),
        (async () => {
            const results = (
                await searchExternalMusicApi(
                    query,
                    'tidal',
                    offset,
                    limit,
                    signal,
                )
            ).results;
            allResults.push(...allResults);
            onResults?.(results, allResults, 'TIDAL');
        })(),
        (async () => {
            const results = (
                await searchExternalMusicApi(
                    query,
                    'qobuz',
                    offset,
                    limit,
                    signal,
                )
            ).results;
            allResults.push(...allResults);
            onResults?.(results, allResults, 'QOBUZ');
        })(),
        (async () => {
            const results = (
                await searchExternalMusicApi(query, 'yt', offset, limit, signal)
            ).results;
            allResults.push(...allResults);
            onResults?.(results, allResults, 'YT');
        })(),
    ]);

    return allResults;
}

async function getArtistFromTidalArtistId(
    tidalArtistId: string,
    signal?: AbortSignal | null,
): Promise<Api.Artist> {
    const con = getConnection();
    const query = new QueryParams({
        artistId: `${tidalArtistId}`,
        source: 'TIDAL',
    });

    return await requestJson(`${con.apiUrl}/menu/artist?${query}`, {
        signal: signal ?? null,
    });
}

async function getArtistFromQobuzArtistId(
    qobuzArtistId: string,
    signal?: AbortSignal | null,
): Promise<Api.Artist> {
    const con = getConnection();
    const query = new QueryParams({
        artistId: `${qobuzArtistId}`,
        source: 'QOBUZ',
    });

    return await requestJson(`${con.apiUrl}/menu/artist?${query}`, {
        signal: signal ?? null,
    });
}

async function getArtistFromTidalAlbumId(
    tidalAlbumId: string,
    signal?: AbortSignal | null,
): Promise<Api.Artist> {
    const con = getConnection();
    const query = new QueryParams({
        albumId: `${tidalAlbumId}`,
        source: 'TIDAL',
    });

    return await requestJson(`${con.apiUrl}/menu/artist?${query}`, {
        signal: signal ?? null,
    });
}

async function getTidalArtist(
    tidalArtistId: string,
    signal?: AbortSignal | null,
): Promise<Api.Artist> {
    const con = getConnection();
    const query = new QueryParams({
        artistId: `${tidalArtistId}`,
        source: 'TIDAL',
    });

    return await requestJson(`${con.apiUrl}/menu/artist?${query}`, {
        signal: signal ?? null,
    });
}

async function getQobuzArtist(
    qobuzArtistId: string,
    signal?: AbortSignal | null,
): Promise<Api.Artist> {
    const con = getConnection();
    const query = new QueryParams({
        artistId: `${qobuzArtistId}`,
        source: 'QOBUZ',
    });

    return await requestJson(`${con.apiUrl}/menu/artist?${query}`, {
        signal: signal ?? null,
    });
}

export function sortAlbumsByDateDesc<T extends { dateReleased: string }>(
    albums: T[],
): T[] {
    return albums.toSorted((a, b) => {
        if (!a.dateReleased) return 1;
        if (!b.dateReleased) return -1;

        return b.dateReleased.localeCompare(a.dateReleased);
    });
}

async function getAlbumFromTidalAlbumId(
    tidalAlbumId: string,
    signal?: AbortSignal | null,
): Promise<Api.Album> {
    const con = getConnection();
    const query = new QueryParams({
        tidalAlbumId: `${tidalAlbumId}`,
    });

    return await requestJson(`${con.apiUrl}/menu/album?${query}`, {
        signal: signal ?? null,
    });
}

async function getAlbumFromQobuzAlbumId(
    qobuzAlbumId: string,
    signal?: AbortSignal | null,
): Promise<Api.Album> {
    const con = getConnection();
    const query = new QueryParams({
        qobuzAlbumId: `${qobuzAlbumId}`,
    });

    return await requestJson(`${con.apiUrl}/menu/album?${query}`, {
        signal: signal ?? null,
    });
}

async function getLibraryAlbumsFromTidalArtistId(
    tidalArtistId: string,
    signal?: AbortSignal | null,
): Promise<Api.Album[]> {
    const con = getConnection();
    const query = new QueryParams({
        tidalArtistId: `${tidalArtistId}`,
    });

    return await requestJson(`${con.apiUrl}/menu/albums?${query}`, {
        signal: signal ?? null,
    });
}

async function getLibraryAlbumsFromQobuzArtistId(
    qobuzArtistId: string,
    signal?: AbortSignal | null,
): Promise<Api.Album[]> {
    const con = getConnection();
    const query = new QueryParams({
        qobuzArtistId: `${qobuzArtistId}`,
    });

    return await requestJson(`${con.apiUrl}/menu/albums?${query}`, {
        signal: signal ?? null,
    });
}

async function getTidalAlbum(
    tidalAlbumId: string,
    signal?: AbortSignal | null,
): Promise<Api.Album> {
    const con = getConnection();
    const query = new QueryParams({
        albumId: `${tidalAlbumId}`,
    });

    return await requestJson(`${con.apiUrl}/tidal/albums?${query}`, {
        signal: signal ?? null,
    });
}

async function getQobuzAlbum(
    qobuzAlbumId: string,
    signal?: AbortSignal | null,
): Promise<Api.Album> {
    const con = getConnection();
    const query = new QueryParams({
        albumId: `${qobuzAlbumId}`,
    });

    return await requestJson(`${con.apiUrl}/qobuz/albums?${query}`, {
        signal: signal ?? null,
    });
}

async function getTidalAlbumTracks(
    tidalAlbumId: string,
    signal?: AbortSignal | null,
): Promise<Api.PagingResponse<Api.Track>> {
    const con = getConnection();
    const query = new QueryParams({
        albumId: `${tidalAlbumId}`,
    });

    return await requestJson(`${con.apiUrl}/tidal/albums/tracks?${query}`, {
        signal: signal ?? null,
    });
}

async function getQobuzAlbumTracks(
    qobuzAlbumId: string,
    signal?: AbortSignal | null,
): Promise<Api.PagingResponse<Api.Track>> {
    const con = getConnection();
    const query = new QueryParams({
        albumId: `${qobuzAlbumId}`,
    });

    return await requestJson(`${con.apiUrl}/qobuz/albums/tracks?${query}`, {
        signal: signal ?? null,
    });
}

async function getYtAlbumTracks(
    ytAlbumId: string,
    signal?: AbortSignal | null,
): Promise<Api.PagingResponse<Api.Track>> {
    const con = getConnection();
    const query = new QueryParams({
        albumId: `${ytAlbumId}`,
    });

    return await requestJson(`${con.apiUrl}/yt/albums/tracks?${query}`, {
        signal: signal ?? null,
    });
}

async function getTidalTrack(
    tidalTrackId: string,
    signal?: AbortSignal | null,
): Promise<Api.Track> {
    const con = getConnection();
    const query = new QueryParams({
        trackId: `${tidalTrackId}`,
    });

    return await requestJson(`${con.apiUrl}/tidal/track?${query}`, {
        signal: signal ?? null,
    });
}

async function getTrackUrlForSource(
    trackId: Id,
    source: ApiSource,
    audioQuality: Api.TrackAudioQuality,
    signal?: AbortSignal | null,
): Promise<string> {
    const con = getConnection();
    const query = new QueryParams({
        quality: audioQuality,
        trackId: `${trackId}`,
        source: `${source}`,
    });

    const urls = await requestJson<string[]>(
        `${con.apiUrl}/files/tracks/url?${query}`,
        {
            signal: signal ?? null,
        },
    );

    return urls[0]!;
}

async function addAlbumToLibrary(
    albumId: {
        tidalAlbumId?: string;
        qobuzAlbumId?: string;
    },
    signal?: AbortSignal | null,
): Promise<void> {
    const con = getConnection();
    const query = new QueryParams({
        albumId: albumId.tidalAlbumId?.toString() ?? albumId.qobuzAlbumId,
        source: albumId.tidalAlbumId
            ? 'TIDAL'
            : albumId.qobuzAlbumId
              ? 'QOBUZ'
              : undefined,
    });

    return await requestJson(`${con.apiUrl}/menu/album?${query}`, {
        method: 'POST',
        signal: signal ?? null,
    });
}

async function removeAlbumFromLibrary(
    albumId: {
        tidalAlbumId?: string;
        qobuzAlbumId?: string;
    },
    signal?: AbortSignal | null,
): Promise<Api.Album> {
    const con = getConnection();
    const query = new QueryParams({
        albumId: albumId.tidalAlbumId?.toString() ?? albumId.qobuzAlbumId,
        source: albumId.tidalAlbumId
            ? 'TIDAL'
            : albumId.qobuzAlbumId
              ? 'QOBUZ'
              : undefined,
    });

    return await requestJson(`${con.apiUrl}/menu/album?${query}`, {
        method: 'DELETE',
        signal: signal ?? null,
    });
}

async function refavoriteAlbum(
    albumId: {
        tidalAlbumId?: string;
        qobuzAlbumId?: string;
    },
    signal?: AbortSignal | null,
): Promise<Api.Album> {
    const con = getConnection();
    const query = new QueryParams({
        albumId: albumId.tidalAlbumId?.toString() ?? albumId.qobuzAlbumId,
        source: albumId.tidalAlbumId
            ? 'TIDAL'
            : albumId.qobuzAlbumId
              ? 'QOBUZ'
              : undefined,
    });

    return await requestJson(`${con.apiUrl}/menu/album/re-favorite?${query}`, {
        method: 'POST',
        signal: signal ?? null,
    });
}

async function retryDownload(
    taskId: number,
    signal?: AbortSignal | null,
): Promise<void> {
    const con = getConnection();
    const query = new QueryParams({
        taskId: `${taskId}`,
    });

    return await requestJson(
        `${con.apiUrl}/downloader/retry-download?${query}`,
        {
            method: 'POST',
            signal: signal ?? null,
        },
    );
}

async function download(
    items: {
        trackId?: string | number;
        trackIds?: (string | number)[];
        albumId?: string | number;
        albumIds?: (string | number)[];
    },
    source: Api.DownloadApiSource,
    signal?: AbortSignal | null,
): Promise<void> {
    const con = getConnection();
    const query = new QueryParams({
        trackId: items.trackId ? `${items.trackId}` : undefined,
        trackIds: items.trackIds ? `${items.trackIds.join(',')}` : undefined,
        albumId: items.albumId ? `${items.albumId}` : undefined,
        albumIds: items.albumIds ? `${items.albumIds.join(',')}` : undefined,
        source: `${source}`,
        locationId: defaultDownloadLocation.get()
            ? `${defaultDownloadLocation.get()}`
            : undefined,
    });

    return await requestJson(`${con.apiUrl}/downloader/download?${query}`, {
        method: 'POST',
        signal: signal ?? null,
    });
}

async function getDownloadTasks(
    signal?: AbortSignal | null,
): Promise<Api.PagingResponseWithTotal<Api.DownloadTask>> {
    const con = getConnection();
    return await requestJson(`${con.apiUrl}/downloader/download-tasks`, {
        signal: signal ?? null,
    });
}

async function getDownloadLocations(
    signal?: AbortSignal | null,
): Promise<Api.PagingResponseWithTotal<Api.DownloadLocation>> {
    const con = getConnection();
    return await requestJson(`${con.apiUrl}/downloader/download-locations`, {
        signal: signal ?? null,
    });
}

async function addDownloadLocation(
    path: string,
    signal?: AbortSignal | null,
): Promise<Api.DownloadLocation> {
    const con = getConnection();
    const query = new QueryParams({
        path: `${path}`,
    });

    return await requestJson(
        `${con.apiUrl}/downloader/download-locations?${query}`,
        {
            method: 'POST',
            signal: signal ?? null,
        },
    );
}

async function getTrackVisualization(
    track: Api.Track | number,
    source: ApiSource,
    max: number,
    signal?: AbortSignal | null,
): Promise<number[]> {
    const con = getConnection();
    const query = new QueryParams({
        trackId: `${trackId(track)}`,
        max: `${Math.ceil(max)}`,
        source: `${source}`,
    });

    return await requestJson(
        `${con.apiUrl}/files/track/visualization?${query}`,
        {
            signal: signal ?? null,
        },
    );
}

async function getAudioZones(
    signal?: AbortSignal | null,
): Promise<Api.PagingResponseWithTotal<Api.AudioZone>> {
    const con = getConnection();
    const query = new QueryParams({ offset: `0`, limit: `100` });

    return await requestJson(`${con.apiUrl}/audio-zone?${query}`, {
        signal: signal ?? null,
    });
}

async function createAudioZone(
    name: string,
    signal?: AbortSignal | null,
): Promise<Api.AudioZone> {
    const con = getConnection();
    const query = new QueryParams({ name });

    return await requestJson(`${con.apiUrl}/audio-zone?${query}`, {
        method: 'POST',
        signal: signal ?? null,
    });
}

async function updateAudioZone(
    update: Api.UpdateAudioZone,
    signal?: AbortSignal | null,
): Promise<Api.AudioZone> {
    const con = getConnection();

    return await requestJson(`${con.apiUrl}/audio-zone`, {
        method: 'PATCH',
        body: JSON.stringify(update),
        signal: signal ?? null,
    });
}

async function deleteAudioZone(
    id: number,
    signal?: AbortSignal | null,
): Promise<void> {
    const con = getConnection();
    const query = new QueryParams({ id: `${id}` });

    return await requestJson(`${con.apiUrl}/audio-zone?${query}`, {
        method: 'DELETE',
        signal: signal ?? null,
    });
}

async function runScan(
    origins: ScanOrigin[],
    signal?: AbortSignal | null,
): Promise<void> {
    const con = getConnection();
    const query = new QueryParams({ origins: `${origins.join(',')}` });

    return await requestJson(`${con.apiUrl}/scan/run-scan?${query}`, {
        method: 'POST',
        signal: signal ?? null,
    });
}

async function startScan(
    origins: ScanOrigin[],
    signal?: AbortSignal | null,
): Promise<void> {
    const con = getConnection();
    const query = new QueryParams({ origins: `${origins.join(',')}` });

    return await requestJson(`${con.apiUrl}/scan/start-scan?${query}`, {
        method: 'POST',
        signal: signal ?? null,
    });
}

async function enableScanOrigin(
    origin: ScanOrigin,
    signal?: AbortSignal | null,
): Promise<void> {
    const con = getConnection();
    const query = new QueryParams({ origin: `${origin}` });

    return await requestJson(`${con.apiUrl}/scan/scan-origins?${query}`, {
        method: 'POST',
        signal: signal ?? null,
    });
}

async function addScanPath(
    path: string,
    signal?: AbortSignal | null,
): Promise<void> {
    const con = getConnection();
    const query = new QueryParams({ path: `${path}` });

    return await requestJson(`${con.apiUrl}/scan/scan-paths?${query}`, {
        method: 'POST',
        signal: signal ?? null,
    });
}

async function getScanPaths(
    signal?: AbortSignal | null,
): Promise<Api.ScanPaths> {
    const con = getConnection();

    return await requestJson(`${con.apiUrl}/scan/scan-paths`, {
        signal: signal ?? null,
    });
}

async function getProfiles(
    connection?: Connection,
    signal?: AbortSignal | null,
): Promise<Api.Profile[]> {
    const con = connection ?? getConnection();

    return await requestJson(`${con.apiUrl}/config/profiles`, {
        signal: signal ?? null,
    });
}

class RequestError extends Error {
    constructor(public response: Response) {
        let message = `Request failed: ${response.status}`;

        if (response.statusText) {
            message += ` (${response.statusText})`;
        }

        if (response.url) {
            message += ` (url='${response.url}')`;
        }

        if (typeof response.redirected !== 'undefined') {
            message += ` (redirected=${response.redirected})`;
        }

        if (response.headers) {
            message += ` (headers=${objToStr(response.headers)})`;
        }

        if (response.type) {
            message += ` (type=${response.type})`;
        }

        super(message);
    }
}

async function requestJson<T>(
    url: string,
    options: Parameters<typeof fetch>[1],
): Promise<T> {
    const con = getConnection();

    if (url[url.length - 1] === '?') url = url.substring(0, url.length - 1);

    const params = new QueryParams();
    const clientId = con.clientId;

    if (clientId) {
        params.set('clientId', clientId);
    }

    if (params.size > 0) {
        if (url.indexOf('?') > 0) {
            url += `&${params}`;
        } else {
            url += `?${params}`;
        }
    }

    const token = con.staticToken || con.token;
    const headers = {
        'Content-Type': 'application/json',
        ...(options?.headers ?? {}),
    } as { [key: string]: string };

    if (token && !headers.Authorization) {
        headers.Authorization = token;
    }
    if (con.profile) {
        headers['moosicbox-profile'] = con.profile;
    }

    options = {
        credentials: 'include',
        ...options,
        headers,
    };

    const response = await fetch(url, options);

    if (!response.ok) {
        throw new RequestError(response);
    }

    return await response.json();
}

export function cancellable<T>(func: (signal: AbortSignal) => Promise<T>): {
    data: Promise<T>;
    controller: AbortController;
    signal: AbortSignal;
} {
    const controller = new AbortController();
    const signal = controller.signal;

    return { data: func(signal), controller, signal };
}

const abortControllers: { [id: string]: AbortController } = {};

export async function once<T>(
    id: string,
    func: (signal: AbortSignal) => Promise<T>,
): Promise<T> {
    const controller = abortControllers[id];

    if (controller) {
        controller.abort();
    }

    const resp = cancellable(func);
    abortControllers[id] = resp.controller;

    let data: T;

    try {
        data = await resp.data;
    } catch (e) {
        throw e;
    } finally {
        delete abortControllers[id];
    }

    return data;
}

export const api: ApiType = {
    getArtist,
    getArtistCover,
    getArtistSourceCover,
    getAlbum,
    getAlbums,
    getAllAlbums,
    getAlbumArtwork,
    getAlbumSourceArtwork,
    getAlbumTracks,
    getAlbumVersions,
    getTracks,
    getArtists,
    fetchSignatureToken,
    refetchSignatureToken,
    validateSignatureTokenAndClient,
    validateSignatureToken,
    magicToken,
    globalSearch,
    searchExternalMusicApi,
    searchAll,
    getArtistFromTidalArtistId,
    getArtistFromQobuzArtistId,
    getArtistFromTidalAlbumId,
    getAlbumFromTidalAlbumId,
    getAlbumFromQobuzAlbumId,
    getTidalArtist,
    getQobuzArtist,
    getLibraryAlbumsFromTidalArtistId,
    getLibraryAlbumsFromQobuzArtistId,
    getTidalAlbum,
    getQobuzAlbum,
    getTidalAlbumTracks,
    getQobuzAlbumTracks,
    getYtAlbumTracks,
    getTidalTrack,
    getTrackUrlForSource,
    addAlbumToLibrary,
    removeAlbumFromLibrary,
    refavoriteAlbum,
    getTrackVisualization,
    retryDownload,
    download,
    getDownloadTasks,
    getDownloadLocations,
    addDownloadLocation,
    getAudioZones,
    createAudioZone,
    updateAudioZone,
    deleteAudioZone,
    runScan,
    startScan,
    enableScanOrigin,
    addScanPath,
    getScanPaths,
    getProfiles,
};
