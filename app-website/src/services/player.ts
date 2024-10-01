import { createSignal } from 'solid-js';
import { Howl } from 'howler';
import { makePersisted } from '@solid-primitives/storage';
import { isServer } from 'solid-js/web';
import {
    type Album,
    Api,
    type Track,
    api,
    connection,
    toSessionPlaylistTrack,
} from './api';
import { createStore, produce } from 'solid-js/store';
import { createListener, deepEqual, orderedEntries } from './util';
import { type PartialBy, type PartialUpdateSession } from './types';
import { wsService } from './ws';
import { appState, showChangePlaybackTargetModal } from './app';
import { responsePromise } from '~/components/ChangePlaybackTargetModal/ChangePlaybackTargetModal';
import { isSilencePlaying, startSilence, stopSilence } from './silence-player';

export type TrackListenerCallback = (track: Track, position: number) => void;

interface PlayerState {
    playing: boolean;
    currentPlaybackSession?: Api.PlaybackSession | undefined;
    playbackSessions: Api.PlaybackSession[];
    currentAudioZone?: Api.AudioZone | undefined;
    audioZones: Api.AudioZone[];
    currentTrack?: Track | undefined;
}

export const [playerState, setPlayerState] = createStore<PlayerState>({
    playing: false,
    currentPlaybackSession: undefined,
    playbackSessions: [],
    currentAudioZone: undefined,
    audioZones: [],
    currentTrack: undefined,
});

export const [_playbackQuality, _setPlaybackQuality] = makePersisted(
    createSignal<Api.PlaybackQuality>(
        { format: Api.AudioFormat.SOURCE },
        { equals: false },
    ),
    {
        name: `player.v1.playbackQuality`,
    },
);
const onPlaybackQualityChangedListener =
    createListener<
        (
            value: ReturnType<typeof _playbackQuality>,
            old: ReturnType<typeof _playbackQuality>,
        ) => boolean | void | Promise<boolean | void>
    >();
export const onPlaybackQualityChanged = onPlaybackQualityChangedListener.on;
export const offPlaybackQualityChanged = onPlaybackQualityChangedListener.off;
export const playbackQuality = _playbackQuality;
export const setPlaybackQuality = (
    value: Parameters<typeof _setPlaybackQuality>[0],
    trigger = true,
) => {
    const old = _playbackQuality();
    if (typeof value === 'function') {
        value = value(old);
    }
    _setPlaybackQuality(value);
    if (trigger && value !== old) {
        onPlaybackQualityChangedListener.trigger(value, old);
    }
    updatePlayback({ quality: value });
};

export const [_currentPlaybackTarget, _setCurrentPlaybackTarget] =
    makePersisted(
        createSignal<Api.PlaybackTarget | undefined>(undefined, {
            equals: false,
        }),
        {
            name: `player.v1.currentPlaybackTarget`,
        },
    );
const onCurrentPlaybackTargetChangedListener =
    createListener<
        (
            value: ReturnType<typeof _currentPlaybackTarget>,
            old: ReturnType<typeof _currentPlaybackTarget>,
        ) => boolean | void | Promise<boolean | void>
    >();
export const onCurrentPlaybackTargetChanged =
    onCurrentPlaybackTargetChangedListener.on;
export const offCurrentPlaybackTargetChanged =
    onCurrentPlaybackTargetChangedListener.off;
export const currentPlaybackTarget = _currentPlaybackTarget;
export const setCurrentPlaybackTarget = (
    value: Parameters<typeof _setCurrentPlaybackTarget>[0],
    trigger = true,
) => {
    const old = _currentPlaybackTarget();
    if (typeof value === 'function') {
        value = value(old);
    }
    _setCurrentPlaybackTarget(value);
    if (trigger && value !== old) {
        onCurrentPlaybackTargetChangedListener.trigger(value, old);
    }
    updatePlayback({});
};

export const [currentPlaybackSessionId, setCurrentPlaybackSessionId] =
    makePersisted(
        createSignal<number | undefined>(undefined, { equals: false }),
        {
            name: `player.v1.currentPlaybackSessionId`,
        },
    );

export const [sound, setSound] = createSignal<Howl>();

const onVolumeChangedListener =
    createListener<(value: number, old: number) => boolean | void>();
export const onVolumeChanged = onVolumeChangedListener.on;
export const offVolumeChanged = onVolumeChangedListener.off;

export function setVolume(volume: number) {
    console.debug('Setting volume to', volume);
    updatePlayback({ volume });
}

export const [_currentSeek, _setCurrentSeek] = makePersisted(
    createSignal<number | undefined>(undefined, { equals: false }),
    {
        name: `player.v1.currentSeek`,
    },
);
const onCurrentSeekChangedListener =
    createListener<
        (
            value: ReturnType<typeof _currentSeek>,
            old: ReturnType<typeof _currentSeek>,
        ) => boolean | void
    >();
export const onCurrentSeekChanged = onCurrentSeekChangedListener.on;
export const offCurrentSeekChanged = onCurrentSeekChangedListener.off;
export const currentSeek = _currentSeek;
export const setCurrentSeek = (
    value: Parameters<typeof _setCurrentSeek>[0],
    trigger = true,
) => {
    const old = _currentSeek();
    if (typeof value === 'function') {
        value = value(old);
    }
    _setCurrentSeek(value);
    if (trigger && value !== old) {
        onCurrentSeekChangedListener.trigger(value, old);
    }
};

export const [_currentTrackLength, _setCurrentTrackLength] = makePersisted(
    createSignal<number>(0, { equals: false }),
    {
        name: `player.v1.currentTrackLength`,
    },
);
const onCurrentTrackLengthChangedListener =
    createListener<
        (
            value: ReturnType<typeof _currentTrackLength>,
            old: ReturnType<typeof _currentTrackLength>,
        ) => boolean | void
    >();
export const onCurrentTrackLengthChanged =
    onCurrentTrackLengthChangedListener.on;
export const offCurrentTrackLengthChanged =
    onCurrentTrackLengthChangedListener.off;
export const currentTrackLength = _currentTrackLength;
export const setCurrentTrackLength = (
    value: Parameters<typeof _setCurrentTrackLength>[0],
    trigger = true,
) => {
    const old = _currentTrackLength();
    if (typeof value === 'function') {
        value = value(old);
    }
    _setCurrentTrackLength(value);
    if (trigger && value !== old) {
        onCurrentTrackLengthChangedListener.trigger(value, old);
    }
};

export const [currentAlbum, setCurrentAlbum] = makePersisted(
    createSignal<Album | Track | undefined>(undefined, {
        equals: false,
    }),
    {
        name: `player.v2.currentAlbum`,
    },
);

export const [_playlistPosition, _setPlaylistPosition] = makePersisted(
    createSignal<number | undefined>(undefined, { equals: false }),
    { name: `player.v1.playlistPosition` },
);
const onPlaylistPositionChangedListener =
    createListener<
        (
            value: ReturnType<typeof _playlistPosition>,
            old: ReturnType<typeof _playlistPosition>,
        ) => boolean | void
    >();
export const onPlaylistPositionChanged = onPlaylistPositionChangedListener.on;
export const offPlaylistPositionChanged = onPlaylistPositionChangedListener.off;
export const playlistPosition = _playlistPosition;
export const setPlaylistPosition = (
    value: Parameters<typeof _setPlaylistPosition>[0],
    trigger = true,
) => {
    const old = _playlistPosition();
    if (typeof value === 'function') {
        value = value(old);
    }
    _setPlaylistPosition(value);
    if (trigger && value !== old) {
        onPlaylistPositionChangedListener.trigger(value, old);
    }
};

const [_playlist, _setPlaylist] = makePersisted(
    createSignal<Track[]>([], { equals: false }),
    { name: `player.v1.playlist` },
);
const onPlaylistChangedListener =
    createListener<
        (
            value: ReturnType<typeof _playlist>,
            old: ReturnType<typeof _playlist>,
        ) => boolean | void
    >();
export const onPlaylistChanged = onPlaylistChangedListener.on;
export const offPlaylistChanged = onPlaylistChangedListener.off;
export const playlist = _playlist;
export const setPlaylist = (
    value: Parameters<typeof _setPlaylist>[0],
    trigger = true,
) => {
    const old = _playlist();
    if (typeof value === 'function') {
        value = value(old);
    }
    _setPlaylist(value);
    if (trigger) {
        onPlaylistChangedListener.trigger(value, old);
    }
};

export interface PlayerType {
    id: number;
    activate?(): boolean | void | Promise<boolean | void>;
    deactivate?(): boolean | void | Promise<boolean | void>;
    updatePlayback(
        update: PlaybackUpdate,
    ): boolean | void | Promise<boolean | void>;
}

const playListener = createListener<() => void>();
export const onPlay = playListener.on;
export const offPlay = playListener.off;

export function isMasterPlayer(zone: Api.AudioZone | undefined): boolean {
    const activeZonePlayers = getActiveZonePlayers(zone);

    console.debug(
        'isMasterPlayer:',
        'zone:',
        zone,
        'players:',
        players,
        'activeZonePlayers:',
        activeZonePlayers,
    );

    if (activeZonePlayers.length === 0) {
        console.debug('isMasterPlayer: no active zone players');
        return false;
    }

    const first = activeZonePlayers[0];

    if (zone?.players.findIndex((p) => p.playerId === first?.id) !== 0) {
        console.debug('isMasterPlayer: player is not first');
        return false;
    }

    console.debug('isMasterPlayer: player is master');
    return true;
}

export function getActiveZonePlayers(
    zone: Api.AudioZone | undefined,
): PlayerType[] {
    console.debug('getActiveZonePlayers: zone =', zone, 'players =', players);
    return (
        players.filter((p) => zone?.players.some((x) => p.id === x.playerId)) ??
        []
    );
}

export function isActiveConnectionPlayer(
    playbackTarget: Api.ConnectionOutputPlaybackTarget,
): boolean {
    const players = getActiveConnectionPlayers(
        appState.connection,
        playbackTarget,
    );

    console.debug('isActiveConnectionPlayer: players =', players);

    return players.length === 1;
}

export function getActiveConnectionPlayers(
    connection: Api.Connection | undefined,
    playbackTarget: Api.ConnectionOutputPlaybackTarget,
): PlayerType[] {
    console.debug(
        'getActiveConnectionPlayers: connection =',
        connection,
        'players =',
        players,
        'playbackTarget =',
        playbackTarget,
    );

    if (connection?.connectionId !== playbackTarget.connectionId) {
        return [];
    }

    return (
        players.filter((p) =>
            connection?.players.some(
                (x) =>
                    p.id === x.playerId &&
                    x.audioOutputId === playbackTarget.outputId,
            ),
        ) ?? []
    );
}

export async function play() {
    console.debug('Play called');
    await updatePlayback({ playing: true });
}

const seekListener = createListener<(seek: number, manual: boolean) => void>();
export const onSeek = seekListener.on;
export const offSeek = seekListener.off;

export async function seek(seek: number, manual = false) {
    console.debug('Seek called');
    if (typeof seek === 'number' && manual) {
        console.debug(`Setting seek to ${seek}`);
        await updatePlayback({ play: playing(), seek });
    }
    seekListener.trigger(seek, manual);
}

const pauseListener = createListener<() => void>();
export const onPause = pauseListener.on;
export const offPause = pauseListener.off;

export async function pause() {
    console.debug('Pause called');
    await updatePlayback({ playing: false });
}

const prevTrackListener = createListener<TrackListenerCallback>();
export const onPreviousTrack = prevTrackListener.on;
export const offPreviousTrack = prevTrackListener.off;

export async function previousTrack(): Promise<boolean> {
    if (playlistPosition() === 0) {
        console.debug('Setting track position to 0');
        seek(0, true);
    } else if ((currentSeek() ?? 0) < 5) {
        console.debug('Playing previous track');

        const position = playlistPosition() ?? 0;
        const newPosition = position > 0 ? position - 1 : position;

        await updatePlayback({
            play: true,
            seek: 0,
            position: newPosition,
        });
        prevTrackListener.trigger(playerState.currentTrack!, newPosition);
    } else {
        console.debug('Setting track position to 0');
        seek(0, true);
    }
    return false;
}

const nextTrackListener = createListener<TrackListenerCallback>();
export const onNextTrack = nextTrackListener.on;
export const offNextTrack = nextTrackListener.off;

export async function nextTrack(): Promise<boolean> {
    if (
        typeof playlistPosition() === 'number' &&
        playlistPosition()! < playlist()!.length - 1
    ) {
        console.debug('Playing next track');

        const position = playlistPosition() ?? 0;
        const newPosition = position + 1;

        await updatePlayback({
            play: true,
            seek: 0,
            position: newPosition,
        });
        nextTrackListener.trigger(playerState.currentTrack!, newPosition);
    } else {
        console.debug('No next track to play');
        stop();
    }

    return false;
}

const stopListener = createListener<() => void>();
export const onStop = stopListener.on;
export const offStop = stopListener.off;

export async function stop() {
    await updatePlayback({ stop: false });
}

const playAlbumListener = createListener<() => void>();
export const onPlayAlbum = playAlbumListener.on;
export const offPlayAlbum = playAlbumListener.off;

export async function playAlbum(album: Album | Track) {
    console.debug('playAlbum', album);
    setCurrentAlbum(album);

    const albumType = 'type' in album ? album.type : 'TRACK';

    switch (albumType) {
        case 'LIBRARY': {
            album = album as Api.LibraryAlbum;
            const versions = await api.getAlbumVersions(album.albumId);
            const tracks = versions[0]!.tracks;
            await playPlaylist(tracks);
            break;
        }
        case 'TRACK': {
            album = album as Api.LibraryTrack;
            const versions = await api.getAlbumVersions(album.albumId);
            const tracks = versions[0]!.tracks;
            await playPlaylist(tracks);
            break;
        }
        case 'TIDAL': {
            album = album as Api.TidalAlbum;
            const page = await api.getTidalAlbumTracks(album.id);
            const tracks = page.items;
            await playPlaylist(tracks);
            break;
        }
        case 'QOBUZ': {
            album = album as Api.QobuzAlbum;
            const page = await api.getQobuzAlbumTracks(album.id);
            const tracks = page.items;
            await playPlaylist(tracks);
            break;
        }
        case 'YT': {
            album = album as Api.YtAlbum;
            const page = await api.getYtAlbumTracks(album.id);
            const tracks = page.items;
            await playPlaylist(tracks);
            break;
        }
        default:
            albumType satisfies never;
            throw new Error(`Invalid album type '${albumType}'`);
    }
}

const playPlaylistListener = createListener<() => void>();
export const onPlayPlaylist = playPlaylistListener.on;
export const offPlayPlaylist = playPlaylistListener.off;

export async function playPlaylist(tracks: Track[]) {
    console.debug('playPlaylist', tracks);
    const firstTrack = tracks[0];
    setCurrentAlbum(firstTrack);

    await updatePlayback({
        play: true,
        position: 0,
        seek: 0,
        tracks,
        quality: playbackQuality(),
    });
}

const addAlbumToQueueListener = createListener<() => void>();
export const onAddAlbumToQueue = addAlbumToQueueListener.on;
export const offAddAlbumToQueue = addAlbumToQueueListener.off;

export async function addAlbumToQueue(album: Album | Track) {
    console.debug('addAlbumToQueue', album);

    const albumType = 'type' in album ? album.type : 'TRACK';

    switch (albumType) {
        case 'LIBRARY': {
            album = album as Api.LibraryAlbum;
            const versions = await api.getAlbumVersions(album.albumId);
            const tracks = versions[0]!.tracks;
            return addTracksToQueue(tracks);
        }
        case 'TRACK': {
            album = album as Api.LibraryTrack;
            const versions = await api.getAlbumVersions(album.albumId);
            const tracks = versions[0]!.tracks;
            return addTracksToQueue(tracks);
        }
        case 'TIDAL': {
            album = album as Api.TidalAlbum;
            const page = await api.getTidalAlbumTracks(album.id);
            const tracks = page.items;
            return addTracksToQueue(tracks);
        }
        case 'QOBUZ': {
            album = album as Api.QobuzAlbum;
            const page = await api.getQobuzAlbumTracks(album.id);
            const tracks = page.items;
            return addTracksToQueue(tracks);
        }
        case 'YT': {
            album = album as Api.YtAlbum;
            const page = await api.getYtAlbumTracks(album.id);
            const tracks = page.items;
            return addTracksToQueue(tracks);
        }
        default:
            albumType satisfies never;
            throw new Error(`Invalid album type '${albumType}'`);
    }
}

export async function addTracksToQueue(tracks: Track[]) {
    console.debug('addTracksToQueue', tracks);
    updatePlayback({
        tracks: [...playlist(), ...tracks],
    });
}

export function removeTrackFromPlaylist(index: number) {
    console.debug('Removing track from playlist', index);

    const update: Parameters<typeof updatePlayback>[0] = {
        tracks: [...playlist()!.filter((_, i) => i !== index)],
    };

    const currentPosition = playlistPosition()!;

    if (index < currentPosition) {
        update.position = currentPosition - 1;
    } else if (index === currentPosition) {
        update.seek = 0;
        update.play = true;
    }

    updatePlayback(update);
}

export function playFromPlaylistPosition(index: number) {
    console.debug('Playing from playlist position', index);
    updatePlayback({ play: true, position: index, seek: 0 });
}

export const players: PlayerType[] = [];

export function containsPlayer(id: number): boolean {
    return players.some((p) => p.id === id);
}

export function registerPlayer(player: PlayerType) {
    if (players.find((p) => p.id === player.id)) {
        console.debug('Player already registered', player);
        return;
    }
    console.debug('Registering player', player);

    players.push(player);
}

export function sessionUpdated(update: PartialUpdateSession) {
    const playbackTarget = update.playbackTarget;
    const playbackTargetType = playbackTarget.type;

    switch (playbackTargetType) {
        case 'AUDIO_ZONE':
            {
                if (
                    !isMasterPlayer(
                        playerState.audioZones.find(
                            (z) => z.id === playbackTarget.audioZoneId,
                        ),
                    )
                ) {
                    handlePlaybackUpdate(update);
                    console.debug('Not master player. Returning');
                    return;
                }
            }
            break;
        case 'CONNECTION_OUTPUT':
            if (!isActiveConnectionPlayer(playbackTarget)) {
                handlePlaybackUpdate(update);
                console.debug('Not active connection player. Returning');
                return;
            }
            break;
        default:
            playbackTargetType satisfies never;
            throw new Error(
                `Invalid playbackTargetType: '${playbackTargetType}'`,
            );
    }

    const sessionId = update.sessionId;
    const profile = update.profile;

    const playbackUpdate: PlaybackUpdate = {
        sessionId,
        profile,
        playbackTarget,
    };

    for (const [key, value] of orderedEntries(update, [
        'play',
        'stop',
        'playing',
        'playlist',
        'position',
        'seek',
        'volume',
    ])) {
        if (typeof value === 'undefined') continue;

        switch (key) {
            case 'play':
                playbackUpdate.play = value;
                break;
            case 'stop':
                playbackUpdate.stop = value;
                break;
            case 'playing':
                playbackUpdate.playing = value;
                break;
            case 'playlist':
                playbackUpdate.tracks = value?.tracks;
                break;
            case 'position':
                playbackUpdate.position = value;
                break;
            case 'seek':
                playbackUpdate.seek = value;
                break;
            case 'volume':
                playbackUpdate.volume = value;
                break;
            case 'quality':
                playbackUpdate.quality = value;
                break;
            case 'active':
            case 'name':
            case 'sessionId':
            case 'profile':
            case 'playbackTarget':
                break;
            default:
                key satisfies never;
        }
    }

    updatePlayback(playbackUpdate, false);
}

async function confirmChangePlaybackTarget() {
    showChangePlaybackTargetModal.set(true);

    // eslint-disable-next-line no-async-promise-executor
    return new Promise(async (resolve) => {
        resolve(await responsePromise());
    });
}

export type PlaybackUpdate = {
    sessionId: number;
    profile: string;
    playbackTarget: Api.PlaybackTarget;
    play?: boolean;
    stop?: boolean;
    playing?: boolean;
    quality?: Api.PlaybackQuality;
    position?: number;
    seek?: number;
    volume?: number;
    tracks?: Track[];
};

async function updatePlayback(
    update: Omit<PlaybackUpdate, 'sessionId' | 'profile' | 'playbackTarget'>,
    updateSession = true,
) {
    if (!update.quality) {
        update.quality = playbackQuality();
    }

    const playbackUpdate = update as PlaybackUpdate;
    const sessionId = playbackUpdate.sessionId ?? currentPlaybackSessionId();
    const profile = playbackUpdate.profile ?? connection.get()?.profile;
    if (!profile) throw new Error('Missing profile');
    const session = playerState.playbackSessions.find(
        (x) => x.sessionId === sessionId,
    );
    let playbackTarget = playbackUpdate.playbackTarget;
    const currentTarget = currentPlaybackTarget();
    let useDefaultPlaybackTarget = false;

    if (session) {
        playbackTarget = session.playbackTarget;
        console.debug(
            'updatePlayback: using session playback target',
            playbackTarget,
        );

        if (
            currentTarget &&
            !deepEqual(currentTarget, playbackTarget) &&
            !session.playing &&
            (update.playing || update.play) &&
            (await confirmChangePlaybackTarget())
        ) {
            useDefaultPlaybackTarget = true;
        }
    } else {
        console.debug(
            'updatePlayback: using passed playback target',
            playbackTarget,
        );
    }

    if (useDefaultPlaybackTarget) {
        if (currentTarget) {
            playbackTarget = currentTarget;
            console.debug(
                'updatePlayback: using default playback target',
                playbackTarget,
            );
        }
    }

    if (updateSession) {
        const sessionUpdate: Parameters<typeof updatePlaybackSession>[1] = {
            sessionId,
            profile,
            playbackTarget,
        };

        for (const [key, value] of orderedEntries(update, [
            'play',
            'playing',
            'position',
            'seek',
            'volume',
            'tracks',
            'quality',
        ])) {
            if (typeof value === 'undefined') continue;

            switch (key) {
                case 'play':
                    sessionUpdate.play = value;
                    if (update.play) {
                        sessionUpdate.playing = true;
                    }
                    break;
                case 'stop':
                    sessionUpdate.stop = value;
                    break;
                case 'playing':
                    sessionUpdate.playing = value;
                    break;
                case 'position':
                    sessionUpdate.position = value;
                    break;
                case 'seek':
                    sessionUpdate.seek = value;
                    break;
                case 'volume':
                    sessionUpdate.volume = value;
                    break;
                case 'tracks':
                    sessionUpdate.playlist = {
                        tracks: value,
                    };
                    break;
                case 'quality':
                    sessionUpdate.quality = value;
                    break;
                default:
                    key satisfies never;
            }
        }

        updatePlaybackSession(sessionId, sessionUpdate);
    }

    const activePlayers = [];
    const playbackTargetType = playbackTarget.type;

    switch (playbackTargetType) {
        case 'AUDIO_ZONE':
            activePlayers.push(
                ...getActiveZonePlayers(
                    playerState.audioZones.find(
                        ({ id }) => id === playbackTarget.audioZoneId,
                    ),
                ),
            );
            break;
        case 'CONNECTION_OUTPUT':
            activePlayers.push(
                ...getActiveConnectionPlayers(
                    appState.connection,
                    playbackTarget,
                ),
            );
            break;
        default:
            playbackTargetType satisfies never;
            throw new Error(
                `Invalid playbackTargetType: '${playbackTargetType}'`,
            );
    }

    console.debug('activePlayers:', activePlayers);

    await updateActivePlayers(activePlayers, {
        ...update,
        sessionId,
        profile,
        playbackTarget,
    });
}

async function updateActivePlayers(
    activePlayers: PlayerType[],
    update: PlaybackUpdate,
) {
    if (activePlayers.length === 0) {
        handlePlaybackUpdate(update);
    } else {
        stopSilence();
    }

    await Promise.all(
        activePlayers.map((activePlayer) =>
            activePlayer.updatePlayback(update),
        ),
    );
}

function handlePlaybackUpdate(update: PlaybackUpdate) {
    for (const [key, value] of orderedEntries(update, [
        'stop',
        'volume',
        'seek',
        'play',
        'tracks',
        'position',
        'playing',
        'quality',
    ])) {
        if (typeof value === 'undefined') continue;

        switch (key) {
            case 'stop':
                if (update.play || update.playing) continue;
                if (navigator.mediaSession) {
                    navigator.mediaSession.playbackState = 'paused';
                }
                break;
            case 'playing':
                if (update.play) continue;
                if (navigator.mediaSession) {
                    navigator.mediaSession.playbackState = update.playing
                        ? 'playing'
                        : 'paused';
                }
                if (update.playing) {
                    return startSilence();
                }
                break;
            case 'play':
                if (!isSilencePlaying()) {
                    return startSilence();
                }
                if (navigator.mediaSession) {
                    navigator.mediaSession.playbackState = 'playing';
                }
                break;
            case 'seek':
                if (typeof update.seek === 'number') {
                    navigator.mediaSession?.setPositionState({
                        position: update.seek,
                        duration: currentTrackLength(),
                    });
                }
                break;
            case 'volume':
            case 'quality':
            case 'tracks':
            case 'position':
            case 'sessionId':
            case 'profile':
            case 'playbackTarget':
                break;
            default:
                key satisfies never;
        }
    }

    const session = playerState.playbackSessions.find(
        (session) => session.sessionId === update.sessionId,
    );

    if (session?.playing) {
        if (!isSilencePlaying()) {
            return startSilence();
        }
    }
}

function updatePlaybackSession(
    id: number,
    request: Omit<PartialBy<PartialUpdateSession, 'playlist'>, 'playlist'> & {
        playlist?: PartialBy<
            Omit<Api.UpdatePlaybackSessionPlaylist, 'tracks'>,
            'sessionPlaylistId'
        > & { tracks: Track[] };
    },
) {
    console.debug('updatePlaybackSession:', id, request);
    setPlayerState(
        produce((state) => {
            const current = state.currentPlaybackSession;
            const session =
                current?.sessionId === id
                    ? current
                    : state.playbackSessions.find((s) => s.sessionId === id);
            if (session) {
                const { playlist } = session;
                if (playlist && request.playlist) {
                    request.playlist.sessionPlaylistId =
                        playlist.sessionPlaylistId;
                }
                updateSessionPartial(state, request as PartialUpdateSession);

                const updatePlaybackSession: Api.UpdatePlaybackSession = {
                    ...request,
                    playlist: undefined,
                } as unknown as Api.UpdatePlaybackSession;

                if (request.playlist) {
                    updatePlaybackSession.playlist = {
                        ...request.playlist,
                        sessionPlaylistId: request.playlist.sessionPlaylistId!,
                        tracks: request.playlist.tracks.map(
                            toSessionPlaylistTrack,
                        ),
                    };

                    console.debug(
                        'updatePlaybackSession: playlist:',
                        updatePlaybackSession.playlist,
                    );
                } else {
                    delete updatePlaybackSession.playlist;
                }

                wsService.updateSession(updatePlaybackSession);
            }
        }),
    );
}

const onCurrentPlaybackSessionChangedListener =
    createListener<
        (
            value: PlayerState['currentPlaybackSession'],
            old: PlayerState['currentPlaybackSession'],
        ) => boolean | void
    >();
export const onCurrentPlaybackSessionChanged =
    onCurrentPlaybackSessionChangedListener.on;
export const offCurrentPlaybackSessionChanged =
    onCurrentPlaybackSessionChangedListener.off;

const onUpdateSessionPartialListener =
    createListener<(value: PartialUpdateSession) => boolean | void>();
export const onUpdateSessionPartial = onUpdateSessionPartialListener.on;
export const offUpdateSessionPartial = onUpdateSessionPartialListener.off;

export function updateSessionPartial(
    state: PlayerState,
    session: PartialUpdateSession,
) {
    state.playbackSessions.forEach((s) => {
        if (s.sessionId === session.sessionId) {
            Object.assign(s, session);
        }
    });

    if (state.currentPlaybackSession?.sessionId === session.sessionId) {
        Object.assign(state.currentPlaybackSession, session);

        let updatedPlaylist = false;

        if (typeof session.seek !== 'undefined') {
            _setCurrentSeek(session.seek);
        }
        if (typeof session.position !== 'undefined') {
            _setPlaylistPosition(session.position);
            updatedPlaylist = true;
        }
        if (typeof session.playlist !== 'undefined') {
            _setPlaylist(session.playlist.tracks);
            updatedPlaylist = true;
        }

        if (updatedPlaylist) {
            if (typeof playlistPosition() === 'number') {
                const track =
                    state.currentPlaybackSession.playlist.tracks[
                        playlistPosition()!
                    ];

                if (track) {
                    state.currentTrack = track;
                    setCurrentTrackLength(Math.round(track.duration));
                }
            } else {
                state.currentTrack = undefined;
                setCurrentTrackLength(0);
            }
        }
    }

    onUpdateSessionPartialListener.trigger(session);
}

export function updateSession(
    state: PlayerState,
    session: Api.PlaybackSession,
    setAsCurrent = false,
) {
    state.playbackSessions.forEach((s) => {
        if (s.sessionId === session.sessionId) {
            Object.assign(s, session);
        }
    });

    if (
        setAsCurrent ||
        session.sessionId === state.currentPlaybackSession?.sessionId
    ) {
        const old = state.currentPlaybackSession;
        state.currentPlaybackSession = session;
        setCurrentPlaybackSessionId(session.sessionId);

        console.debug('session changed to', session, 'from', old);

        _setPlaylist(session.playlist.tracks);
        _setCurrentSeek(session.seek);
        _setPlaylistPosition(
            session.playlist.tracks.length > 0 ? session.position : undefined,
        );

        if (typeof playlistPosition() === 'number') {
            const track = session.playlist.tracks[playlistPosition()!];

            if (track) {
                state.currentTrack = track;
                setCurrentTrackLength(Math.round(track.duration));
            }
        } else {
            state.currentTrack = undefined;
            setCurrentTrackLength(0);
        }

        onCurrentPlaybackSessionChangedListener.trigger(session, old);
    }
}

onCurrentSeekChanged((value, old) => {
    console.debug('current seek changed from', old, 'to', value);
    if (typeof value === 'number') {
        navigator.mediaSession?.setPositionState({
            position: value,
            duration: currentTrackLength(),
        });
    }
    const activeZonePlayer = playerState.audioZones.some((zone) =>
        isMasterPlayer(zone),
    );
    const playbackTarget = currentPlaybackTarget();
    if (
        activeZonePlayer ||
        (playbackTarget?.type === 'CONNECTION_OUTPUT' &&
            isActiveConnectionPlayer(playbackTarget))
    ) {
        updatePlayback({ seek: value ?? 0 });
    }
});

onUpdateSessionPartial((session) => {
    if (playerState.currentPlaybackSession?.sessionId !== session.sessionId) {
        return;
    }

    if (typeof session.seek !== 'undefined') {
        _setCurrentSeek(session.seek);
    }
});

export function playing(): boolean {
    return playerState.currentPlaybackSession?.playing ?? false;
}

if (!isServer) {
    if (navigator?.mediaSession) {
        onCurrentPlaybackSessionChanged((value) => {
            navigator.mediaSession.playbackState = value?.playing
                ? 'playing'
                : 'paused';
            console.debug(
                'updated playback state to',
                navigator.mediaSession.playbackState,
            );
        });
        navigator.mediaSession.setActionHandler('play', () => {
            console.log('mediaSession: play');
            play();
        });
        navigator.mediaSession.setActionHandler('pause', () => {
            console.log('mediaSession: pause');
            if (navigator.mediaSession.playbackState === 'playing') {
                pause();
            } else {
                play();
            }
        });
        navigator.mediaSession.setActionHandler('stop', () => {
            console.log('mediaSession: stop');
            stop();
        });
        navigator.mediaSession.setActionHandler('nexttrack', () => {
            console.log('mediaSession: nexttrack');
            nextTrack();
        });
        navigator.mediaSession.setActionHandler('previoustrack', () => {
            console.log('mediaSession: previoustrack');
            previousTrack();
        });
    }

    document.body.onkeydown = function (e) {
        const target = e.target as HTMLElement;

        if (
            !(target instanceof HTMLInputElement) &&
            (e.key == ' ' || e.code == 'Space')
        ) {
            if (playerState.currentPlaybackSession?.playing || playing()) {
                pause();
            } else {
                play();
            }
            e.preventDefault();
        }
    };
}
