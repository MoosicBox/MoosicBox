import { createSignal } from 'solid-js';
import { Howl } from 'howler';
import type { HowlCallback } from 'howler';
import { Api, api, getConnection } from './api';
import type { Track } from './api';
import {
    currentSeek,
    playing,
    playlist,
    playlistPosition,
    setCurrentSeek,
    setCurrentTrackLength,
    playbackQuality,
    playerState,
} from './player';
import type { PlayerType } from './player';
import * as player from './player';
import { QueryParams, orderedEntries } from './util';

export type TrackListenerCallback = (
    track: Api.LibraryTrack,
    position: number,
) => void;

export const [sound, setSound] = createSignal<Howl>();

export function createPlayer(id: number): PlayerType {
    let howlPlaying = false;

    let seekHandle: NodeJS.Timeout;
    let endHandle: HowlCallback;
    let loadHandle: HowlCallback;

    async function getTrackUrl(track: Track): Promise<string> {
        const trackType = track.type;

        switch (trackType) {
            case 'LIBRARY': {
                const query = new QueryParams({
                    trackId: track.trackId.toString(),
                });

                const con = getConnection();
                const clientIdParam = con.clientId;
                const signatureToken = Api.signatureToken();

                if (con.clientId && signatureToken) {
                    query.set('clientId', clientIdParam);
                    query.set('signature', signatureToken);
                }
                if (con.profile) {
                    query.set('moosicboxProfile', con.profile);
                }
                if (con.staticToken) {
                    query.set('authorization', con.staticToken);
                }

                if (playbackQuality().format !== Api.AudioFormat.SOURCE) {
                    query.set('format', playbackQuality().format);
                }

                return `${con.apiUrl}/files/track?${query}`;
            }
            default:
                return await api.getTrackUrlForSource(
                    track.id,
                    trackType,
                    Api.TrackAudioQuality.Low,
                );
        }
    }

    function refreshCurrentSeek() {
        const seek = sound()?.seek();
        if (typeof seek === 'number') {
            const roundedSeek = Math.round(seek);
            if (currentSeek() !== roundedSeek) {
                console.debug(`Setting currentSeek to ${roundedSeek}`);
                setCurrentSeek(roundedSeek);
            }
        }
    }

    async function setTrack(): Promise<boolean> {
        if (!sound()) {
            if (typeof playlistPosition() === 'undefined') {
                console.debug('No track to play');
                return false;
            }
            const track = playlist()![playlistPosition()!];
            if (!track) {
                console.debug(
                    'Not a valid track at playlist position',
                    playlistPosition(),
                );
                return false;
            }

            console.debug('Setting track to', track);

            let format: string | undefined;

            const trackType = track.type;

            switch (trackType) {
                case 'LIBRARY': {
                    const trackFormat = track.format;
                    switch (trackFormat) {
                        case Api.AudioFormat.AAC:
                            format = 'm4a';
                            break;
                        case Api.AudioFormat.FLAC:
                            format = 'flac';
                            break;
                        case Api.AudioFormat.MP3:
                            format = 'mp3';
                            break;
                        case Api.AudioFormat.OPUS:
                            format = 'opus';
                            break;
                        case Api.AudioFormat.SOURCE:
                            break;
                        default:
                            trackFormat satisfies never;
                            throw new Error(
                                `Invalid track format '${trackFormat}'`,
                            );
                    }
                    break;
                }
                case 'TIDAL':
                case 'QOBUZ':
                case 'YT':
                    format = 'source';
                    break;
                default:
                    trackType satisfies never;
                    throw new Error(`Invalid track type '${trackType}'`);
            }

            const howl = new Howl({
                src: [await getTrackUrl(track)],
                format,
                html5: true,
            });
            howl.volume(playerState.currentPlaybackSession?.volume ?? 1);
            howl.pannerAttr({ panningModel: 'equalpower' });
            setSound(howl);
            const duration = Math.round(track.duration);
            if (!isNaN(duration) && isFinite(duration)) {
                setCurrentTrackLength(duration);
            }
        }
        return true;
    }

    let ended: boolean = true;
    let loaded = false;

    async function play(): Promise<boolean> {
        const initialSeek = !sound() ? currentSeek() : undefined;

        if (!sound() || ended) {
            if (!(await setTrack())) return false;

            sound()!.on(
                'end',
                (endHandle = (id: number) => {
                    if (ended) {
                        console.debug(
                            'End called after track already ended',
                            id,
                            sound(),
                            sound()?.duration(),
                        );
                        return;
                    }
                    console.debug(
                        'Track ended',
                        id,
                        sound(),
                        sound()?.duration(),
                    );
                    ended = true;
                    loaded = false;
                    stop();
                    player.nextTrack();
                }),
            );
            sound()!.on(
                'load',
                (loadHandle = (...args) => {
                    ended = false;
                    loaded = true;
                    console.debug(
                        'Track loaded',
                        sound(),
                        sound()!.duration(),
                        ...args,
                    );
                    const duration = Math.round(sound()!.duration());
                    if (!isNaN(duration) && isFinite(duration)) {
                        setCurrentTrackLength(duration);
                    }
                    if (typeof initialSeek === 'number') {
                        console.debug(`Setting initial seek to ${initialSeek}`);
                        sound()!.seek(initialSeek);
                    }
                }),
            );
        }

        sound()!.play();

        seekHandle = setInterval(() => {
            if (!loaded) return;
            refreshCurrentSeek();
        }, 200);

        if (loaded && typeof initialSeek === 'number') {
            console.debug(`Setting initial seek to ${initialSeek}`);
            sound()!.seek(initialSeek);
        }

        console.debug('Playing', sound());

        return true;
    }

    function seek(seek: number): boolean {
        console.debug('Track seeked', seek);
        sound()?.seek(seek);
        return true;
    }

    function pause(): boolean {
        sound()?.pause();
        clearInterval(seekHandle);
        console.debug('Paused');
        return true;
    }

    function stopHowl() {
        howlPlaying = false;
        sound()?.off('end', endHandle);
        sound()?.off('load', loadHandle);
        if (!ended) {
            sound()?.stop();
        }
        loaded = false;
        sound()?.unload();
        setSound(undefined);
    }

    function stop(): boolean {
        stopHowl();
        clearInterval(seekHandle);
        console.debug('Track stopped');
        return true;
    }

    const onBeforeUnload = () => {
        playerState.audioZones.forEach((zone) => {
            if (player.isMasterPlayer(zone) && playing()) {
                player.pause();
            }
        });
    };

    const self = {
        id,
        updatePlayback(update: player.PlaybackUpdate) {
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
                        stop();
                        break;
                    case 'volume':
                        sound()?.volume(value);
                        break;
                    case 'seek':
                        if (!update.play) continue;
                        seek(value);
                        break;
                    case 'playing':
                        if (value) {
                            if (!howlPlaying && !update.play) {
                                play();
                                howlPlaying = true;
                            }
                        } else if (howlPlaying) {
                            pause();
                            howlPlaying = false;
                        }
                        break;
                    case 'play':
                        if (
                            Object.keys(update).every((k) =>
                                [
                                    'sessionId',
                                    'play',
                                    'playing',
                                    'seek',
                                ].includes(k),
                            ) &&
                            typeof update.seek === 'number' &&
                            sound()
                        ) {
                            if (!howlPlaying) {
                                sound()!.play();
                            }
                            return;
                        }

                        if (sound()) {
                            stop();
                        }
                        play();
                        howlPlaying = true;
                        break;
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
        },
        activate() {
            window.addEventListener('beforeunload', onBeforeUnload);
            import.meta.hot?.on('vite:beforeUpdate', onBeforeUnload);
        },
        deactivate() {
            window.removeEventListener('beforeunload', onBeforeUnload);
            import.meta.hot?.dispose(onBeforeUnload);

            if (sound()) {
                console.debug('stopping howl');
                stopHowl();
            }
        },
    };

    return self;
}
