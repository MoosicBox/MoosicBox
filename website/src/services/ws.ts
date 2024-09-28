import * as player from './player';
import { produce } from 'solid-js/store';
import { Api, connection, toSessionPlaylistTrack } from './api';
import type { Track } from './api';
import { setAppState } from './app';
import type { PartialUpdateSession } from './types';
import { ClientAtom, clientAtom, createListener, objToStr } from './util';
import { onDownloadEventListener } from './downloads';
import type { DownloadEvent } from './downloads';
import { onScanEventListener, ScanEvent } from './scan';

connection.listen((con) => {
    if (!con) return;

    updateWsUrl(
        con.apiUrl,
        con.profile,
        con.clientId,
        Api.signatureToken(),
        con.staticToken,
    );
    if (con.token && !Api.signatureToken()) {
        console.debug('Waiting for signature token');
        return;
    }
    wsService.reconnect();
});
Api.onSignatureTokenUpdated((signatureToken) => {
    const con = connection.get();
    if (!con) return;

    updateWsUrl(
        con.apiUrl,
        con.profile,
        con.clientId,
        signatureToken,
        con.staticToken,
    );
    if (con.token && !signatureToken) {
        console.debug('Waiting for signature token');
        return;
    }
    wsService.reconnect();
});

export const onConnectionChangedListener =
    createListener<
        (value: string) => boolean | void | Promise<boolean | void>
    >();
export const onConnectionChanged = onConnectionChangedListener.on;
export const offConnectionChanged = onConnectionChangedListener.off;

function updateWsUrl(
    apiUrl: string,
    profile: string | undefined,
    clientId: string | undefined,
    signatureToken: string | undefined,
    staticToken: string | undefined,
) {
    if (!apiUrl?.startsWith('http')) return;

    const params = [];
    if (profile) {
        params.push(`moosicboxProfile=${encodeURIComponent(profile)}`);
    }
    if (clientId) {
        params.push(`clientId=${encodeURIComponent(clientId)}`);
    }
    if (signatureToken) {
        params.push(`signature=${encodeURIComponent(signatureToken)}`);
    }
    if (staticToken) {
        params.push(`authorization=${encodeURIComponent(staticToken)}`);
    }
    wsUrl = `ws${apiUrl.slice(4)}/ws${
        params.length > 0 ? `?${params.join('&')}` : ''
    }`;
    onConnectionChangedListener.trigger(wsUrl);
}

let ws: WebSocket;
let wsUrl: string;
export let connectionPromise: Promise<WebSocket>;

export function setWsUrl(url: string) {
    wsUrl = url;
}

export function setConnectionId(key: string, id?: string | undefined) {
    if (!connectionIds[key]) {
        connectionIds[key] = clientAtom<string>(
            '',
            `ws.v2.connectionId.${key}`,
        );
    }

    if (typeof id === 'string' && !connectionIds[key].get()) {
        connectionIds[key].set(id);
    }

    connectionId.set(connectionIds[key].get());
}

const connectionId = clientAtom<string>('', `ws.v2.connectionId`);
const connectionIds: { [url: string]: ClientAtom<string> } = {};
export const $connectionId = () => connectionId.get();

export const connectionName = clientAtom<string>(
    'New Connection',
    'ws.v1.connectionName',
);

export const onConnectListener =
    createListener<
        (value: string) => boolean | void | Promise<boolean | void>
    >();
export const onConnect = onConnectListener.on;
export const offConnect = onConnectListener.off;

onConnect((id) => {
    setConnectionId(`${connection.get()?.id}`, id);
    wsService.getSessions();
});

connection.listen((connection) => {
    setConnectionId(`${connection?.id}`);
});

export enum InboundMessageType {
    CONNECTION_ID = 'CONNECTION_ID',
    SESSIONS = 'SESSIONS',
    SESSION_UPDATED = 'SESSION_UPDATED',
    CONNECTIONS = 'CONNECTIONS',
    SET_SEEK = 'SET_SEEK',
    DOWNLOAD_EVENT = 'DOWNLOAD_EVENT',
    SCAN_EVENT = 'SCAN_EVENT',
}

export enum OutboundMessageType {
    PING = 'PING',
    GET_CONNECTION_ID = 'GET_CONNECTION_ID',
    PLAYBACK_ACTION = 'PLAYBACK_ACTION',
    GET_SESSIONS = 'GET_SESSIONS',
    CREATE_SESSION = 'CREATE_SESSION',
    UPDATE_SESSION = 'UPDATE_SESSION',
    DELETE_SESSION = 'DELETE_SESSION',
    REGISTER_CONNECTION = 'REGISTER_CONNECTION',
    REGISTER_PLAYERS = 'REGISTER_PLAYERS',
    CREATE_AUDIO_ZONE = 'CREATE_AUDIO_ZONE',
    SET_SEEK = 'SET_SEEK',
}

export interface ConnectionIdMessage extends InboundMessage {
    connectionId: string;
    type: InboundMessageType.CONNECTION_ID;
}

export interface SessionsMessage extends InboundMessage {
    type: InboundMessageType.SESSIONS;
    payload: Api.PlaybackSession[];
}

export interface ConnectionsMessage extends InboundMessage {
    type: InboundMessageType.CONNECTIONS;
    payload: Api.Connection[];
}

export interface SessionUpdatedMessage extends InboundMessage {
    type: InboundMessageType.SESSION_UPDATED;
    payload: PartialUpdateSession;
}

export interface SetSeek {
    sessionId: number;
    playbackTarget: Api.PlaybackTarget;
    seek: number;
}

export interface SetSeekInboundMessage extends InboundMessage {
    type: InboundMessageType.SET_SEEK;
    payload: SetSeek;
}

export interface DownloadEventInboundMessage extends InboundMessage {
    type: InboundMessageType.DOWNLOAD_EVENT;
    payload: DownloadEvent;
}

export interface ScanEventInboundMessage extends InboundMessage {
    type: InboundMessageType.SCAN_EVENT;
    payload: ScanEvent;
}

export interface GetConnectionIdMessage extends OutboundMessage {
    type: OutboundMessageType.GET_CONNECTION_ID;
}

export interface PingMessage extends OutboundMessage {
    type: OutboundMessageType.PING;
}

export type RegisterConnection = Omit<Api.Connection, 'players' | 'alive'> & {
    players: RegisterPlayer[];
};
export interface RegisterConnectionMessage extends OutboundMessage {
    type: OutboundMessageType.REGISTER_CONNECTION;
    payload: RegisterConnection;
}

export type RegisterPlayer = Omit<Api.Player, 'playerId'>;
export interface RegisterPlayersMessage extends OutboundMessage {
    type: OutboundMessageType.REGISTER_PLAYERS;
    payload: RegisterPlayer[];
}

export enum PlaybackAction {
    PLAY = 'PLAY',
    PAUSE = 'PAUSE',
    STOP = 'STOP',
    NEXT_TRACK = 'NEXT_TRACK',
    PREVIOUS_TRACK = 'PREVIOUS_TRACK',
}

export interface PlaybackActionMessage extends OutboundMessage {
    type: OutboundMessageType.PLAYBACK_ACTION;
    payload: {
        action: PlaybackAction;
    };
}

export interface GetSessionsMessage extends OutboundMessage {
    type: OutboundMessageType.GET_SESSIONS;
}

export interface CreateAudioZoneMessage extends OutboundMessage {
    type: OutboundMessageType.CREATE_AUDIO_ZONE;
    payload: CreateAudioZoneRequest;
}

export interface CreateAudioZoneRequest {
    name: string;
}

export interface CreateSessionRequest {
    name: string;
    playlist: CreateSessionPlaylistRequest;
    playbackTarget: Api.PlaybackTarget | undefined;
}

export interface CreateSessionPlaylistRequest {
    tracks: Track[];
}

export interface CreateSession {
    name: string;
    playlist: CreateSessionPlaylist;
}

export interface CreateSessionPlaylist {
    tracks: Api.UpdateSessionPlaylistTrack[];
}

export interface CreateSessionMessage extends OutboundMessage {
    type: OutboundMessageType.CREATE_SESSION;
    payload: CreateSession;
}

export interface UpdateSessionMessage extends OutboundMessage {
    type: OutboundMessageType.UPDATE_SESSION;
    payload: Api.UpdatePlaybackSession;
}

export interface DeleteSessionMessage extends OutboundMessage {
    type: OutboundMessageType.DELETE_SESSION;
    payload: { sessionId: number };
}

export interface InboundMessage {
    type: InboundMessageType;
}

export interface OutboundMessage {
    type: OutboundMessageType;
}

export const onMessageListener =
    createListener<
        (
            message: InboundMessage,
        ) => boolean | void | Promise<boolean> | Promise<void>
    >();
export const onMessage = onMessageListener.on;
export const onMessageFirst = onMessageListener.onFirst;
export const offMessage = onMessageListener.off;

onMessageFirst((data) => {
    console.debug('Received ws message', data);
    switch (data.type) {
        case InboundMessageType.CONNECTION_ID: {
            const message = data as ConnectionIdMessage;
            onConnectListener.trigger(message.connectionId);
            break;
        }
        case InboundMessageType.SESSIONS: {
            const message = data as SessionsMessage;
            player.setPlayerState(
                produce((state) => {
                    state.playbackSessions = message.payload;
                    const existing = message.payload.find(
                        (p) =>
                            p.sessionId ===
                            state.currentPlaybackSession?.sessionId,
                    );
                    if (existing) {
                        player.updateSession(state, existing);
                    } else if (
                        typeof player.currentPlaybackSessionId() === 'number'
                    ) {
                        const session =
                            message.payload.find(
                                (s) =>
                                    s.sessionId ===
                                    player.currentPlaybackSessionId(),
                            ) ?? message.payload[0];
                        if (session) {
                            player.updateSession(state, session, true);
                        }
                    } else {
                        player.updateSession(state, message.payload[0]!, true);
                    }
                }),
            );
            break;
        }
        case InboundMessageType.CONNECTIONS: {
            const message = data as ConnectionsMessage;
            setAppState(
                produce((state) => {
                    state.connections = message.payload;
                    state.connection = state.connections.find(
                        (c) => c.connectionId === $connectionId(),
                    );
                }),
            );
            break;
        }
        case InboundMessageType.SET_SEEK: {
            const message = data as SetSeekInboundMessage;
            if (
                message.payload.sessionId ===
                player.playerState.currentPlaybackSession?.sessionId
            ) {
                player.seek(message.payload.seek);
            }
            break;
        }
        case InboundMessageType.DOWNLOAD_EVENT: {
            const message = data as DownloadEventInboundMessage;
            onDownloadEventListener.trigger(message.payload);
            break;
        }
        case InboundMessageType.SCAN_EVENT: {
            const message = data as ScanEventInboundMessage;
            onScanEventListener.trigger(message.payload);
            break;
        }
        case InboundMessageType.SESSION_UPDATED: {
            const message = data as SessionUpdatedMessage;

            const session = message.payload;

            player.setPlayerState(
                produce((state) => {
                    player.updateSessionPartial(state, session);
                }),
            );
            player.sessionUpdated(session);

            break;
        }
    }
});

const MAX_CONNECTION_RETRY_COUNT: number = -1;
const CONNECTION_RETRY_DEBOUNCE = 5000;

const wsContext: {
    lastConnectionAttemptTime: number;
    messageBuffer: OutboundMessage[];
} = {
    lastConnectionAttemptTime: 0,
    messageBuffer: [],
};

export const wsService = {
    ping() {
        this.send<PingMessage>({ type: OutboundMessageType.PING });
    },

    getConnectionId() {
        this.send<GetConnectionIdMessage>({
            type: OutboundMessageType.GET_CONNECTION_ID,
        });
    },

    registerConnection(connection: RegisterConnection) {
        this.send<RegisterConnectionMessage>({
            type: OutboundMessageType.REGISTER_CONNECTION,
            payload: connection,
        });
    },

    registerPlayers(players: RegisterPlayer[]) {
        this.send<RegisterPlayersMessage>({
            type: OutboundMessageType.REGISTER_PLAYERS,
            payload: players,
        });
    },

    playbackAction(action: PlaybackAction) {
        this.send<PlaybackActionMessage>({
            type: OutboundMessageType.PLAYBACK_ACTION,
            payload: {
                action,
            },
        });
    },

    createAudioZone(audioZone: CreateAudioZoneRequest) {
        this.send<CreateAudioZoneMessage>({
            type: OutboundMessageType.CREATE_AUDIO_ZONE,
            payload: {
                ...audioZone,
            },
        });
    },

    getSessions() {
        this.send<GetSessionsMessage>({
            type: OutboundMessageType.GET_SESSIONS,
        });
    },

    activateSession(sessionId: number, profile: string) {
        this.updateSession({ sessionId, profile, active: true });
    },

    createSession(session: CreateSessionRequest) {
        this.send<CreateSessionMessage>({
            type: OutboundMessageType.CREATE_SESSION,
            payload: {
                ...session,
                playlist: {
                    ...session.playlist,
                    tracks: session.playlist.tracks.map(toSessionPlaylistTrack),
                },
            },
        });
    },

    updateSession(session: Api.UpdatePlaybackSession) {
        const payload: Api.UpdatePlaybackSession = {
            ...session,
            playlist: undefined,
        } as unknown as Api.UpdatePlaybackSession;

        if (session.playlist) {
            payload.playlist = {
                ...session.playlist,
            };
        } else {
            delete payload.playlist;
        }

        this.send<UpdateSessionMessage>({
            type: OutboundMessageType.UPDATE_SESSION,
            payload,
        });
    },

    deleteSession(sessionId: number) {
        this.send<DeleteSessionMessage>({
            type: OutboundMessageType.DELETE_SESSION,
            payload: {
                sessionId,
            },
        });
    },

    send<T extends OutboundMessage>(value: T) {
        if (ws) {
            console.debug('Sending WebSocket message', value);
            ws.send(JSON.stringify(value));
        } else {
            console.debug('Adding WebSocket message to buffer', value);
            wsContext.messageBuffer.push(value);
        }
    },

    newClient(): Promise<void> {
        return new Promise((resolve, reject) => {
            console.log('connecting to ', wsUrl);
            const client = new WebSocket(wsUrl);

            let pingInterval: NodeJS.Timeout | undefined;
            let opened = false;

            client.addEventListener('error', (e: Event) => {
                console.error('WebSocket client error', e);
                if (!opened) {
                    client.close();
                    reject();
                }
            });

            client.addEventListener('open', (_e: Event) => {
                const wasOpened = opened;
                opened = true;
                if (!wasOpened) {
                    pingInterval = setInterval(
                        () => {
                            if (!opened) return clearInterval(pingInterval);

                            this.ping();
                        },
                        9 * 60 * 1000,
                    );

                    ws = client;

                    while (wsContext.messageBuffer.length > 0) {
                        const value = wsContext.messageBuffer.shift();
                        console.debug(
                            'Sending buffered WebSocket message',
                            value,
                        );
                        ws.send(JSON.stringify(value));
                    }

                    this.getConnectionId();
                    resolve();
                }
            });

            client.addEventListener(
                'message',
                (event: MessageEvent<string>) => {
                    const data = JSON.parse(event.data) as InboundMessage;
                    onMessageListener.trigger(data);
                },
            );

            client.addEventListener('close', async () => {
                if (opened) {
                    console.debug('Closed WebSocket connection');
                    opened = false;
                    client.close();
                    clearInterval(pingInterval);

                    const now = Date.now();
                    if (wsContext.lastConnectionAttemptTime + 5000 > now) {
                        console.debug(
                            `Debouncing connection retry attempt. Waiting ${CONNECTION_RETRY_DEBOUNCE}ms`,
                        );
                        await this.sleep(CONNECTION_RETRY_DEBOUNCE);
                    }
                    wsContext.lastConnectionAttemptTime = now;
                    await this.attemptConnection();
                } else {
                    reject();
                }
            });
        });
    },

    async sleep(ms: number): Promise<void> {
        return new Promise((resolve) => {
            setTimeout(resolve, ms);
        });
    },

    async attemptConnection(): Promise<void> {
        let attemptNumber = 0;

        // eslint-disable-next-line no-constant-condition
        while (true) {
            console.debug(
                `Attempting connection${
                    attemptNumber > 0 ? `, Attempt ${attemptNumber + 1}` : ''
                }`,
            );

            try {
                await this.newClient();

                console.debug('Successfully connected client');

                return;
            } catch (e: unknown) {
                if (
                    attemptNumber++ === MAX_CONNECTION_RETRY_COUNT &&
                    MAX_CONNECTION_RETRY_COUNT !== -1
                ) {
                    break;
                }

                console.error(
                    `WebSocket connection failed at '${wsUrl}':`,
                    objToStr(e),
                );
                console.debug(
                    `Failed to connect. Waiting ${CONNECTION_RETRY_DEBOUNCE}ms`,
                );
                await this.sleep(CONNECTION_RETRY_DEBOUNCE);
            }
        }

        throw new Error('Failed to establish connection to websocket server');
    },

    reconnect(): Promise<void> {
        if (ws) ws.close();

        return this.attemptConnection();
    },
};
