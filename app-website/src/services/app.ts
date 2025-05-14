import { Api, api, connection, refreshConnectionProfiles } from './api';
import { createSignal } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import { clientAtom } from './util';
import {
    currentPlaybackTarget,
    setCurrentPlaybackTarget,
    setPlayerState,
} from './player';
import { isServer } from 'solid-js/web';

export const navigationBarExpanded = clientAtom<boolean>(
    true,
    'navigationBarExpanded',
);
export const showAudioZones = clientAtom(false);
export const showPlaybackSessions = clientAtom(false);
export const showPlaybackQuality = clientAtom(false);
export const showChangePlaybackTargetModal = clientAtom(false);
export const showScanStatusBanner = clientAtom(false);
export const errorMessages = clientAtom<string[]>([]);

export function addErrorMessage(message: string) {
    const existing = errorMessages.get();

    if (existing.some((x) => x === message)) {
        return;
    }

    errorMessages.set([...existing, message]);
}

export function clearErrorMessages() {
    errorMessages.set([]);
}

if (!isServer) {
    window.addEventListener('error', (e) => {
        let message = e.error.message;

        if (typeof e.error === 'object' && 'error' in e.error) {
            message += '\n' + e.error.stack;
        }
        if ('stack' in e) {
            message += '\n' + e.stack;
        }

        console.error(
            'Error occurred',
            message,
            JSON.stringify(e, ['message', 'arguments', 'type', 'name']),
        );
        addErrorMessage(message);

        return false;
    });

    window.addEventListener('unhandledrejection', (e) => {
        let message = 'Promise Error';

        if (e.reason?.message) {
            message += ': ' + e.reason.message;
        } else if (typeof e === 'object' && 'message' in e && e.message) {
            message += ': ' + e.message;
        } else if (typeof e.reason === 'string') {
            message += ': ' + e.reason;
        }

        if (typeof e.reason === 'object' && 'error' in e.reason) {
            message += '\n' + e.reason.stack;
        }
        if ('stack' in e) {
            message += '\n' + e.stack;
        }

        console.error(
            'Promise Error occurred',
            message,
            JSON.stringify(e, ['message', 'arguments', 'type', 'name']),
        );
        addErrorMessage(message);
    });
}

type StartupCallback = () => void | Promise<void>;

// eslint-disable-next-line no-var
var startedUp: boolean | undefined;

// eslint-disable-next-line no-var
var startupCallbacks: StartupCallback[] | undefined;

function getStartupCallbacks(): StartupCallback[] {
    if (!startupCallbacks) {
        startupCallbacks = [];
    }
    return startupCallbacks;
}

function isStartedUp(): boolean {
    if (typeof startedUp === 'undefined') {
        startedUp = false;
    }
    return startedUp === true;
}

function setStartedUp(value: boolean) {
    startedUp = value;
}

// Make sure startup callbacks run sequentially
let startupQueuePromise: Promise<void> | void | null = null;

function invokeStartupCallback(func: StartupCallback) {
    if (startupQueuePromise) {
        startupQueuePromise = startupQueuePromise.then(() => {
            return func();
        });
        return;
    }
    startupQueuePromise = func();
}

export function onStartupFirst(func: StartupCallback) {
    if (isStartedUp()) {
        invokeStartupCallback(func);
        return;
    }
    getStartupCallbacks().unshift(func);
}

export async function onStartup(func: StartupCallback) {
    if (isStartedUp()) {
        invokeStartupCallback(func);
        return;
    }
    getStartupCallbacks().push(func);
}

export async function triggerStartup() {
    if (isStartedUp()) return;
    setStartedUp(true);

    for (const func of getStartupCallbacks()) {
        try {
            await func();
        } catch (e) {
            console.error('Startup error:', e);
        }
    }
}

interface AppState {
    connections: Api.Connection[];
    connection: Api.Connection | undefined;
}

export const [appState, setAppState] = createStore<AppState>({
    connections: [],
    connection: undefined,
});

export const [currentArtistSearch, setCurrentArtistSearch] = createSignal<{
    query: string;
    results: Api.Artist[];
}>();

export const [currentAlbumSearch, setCurrentAlbumSearch] = createSignal<{
    query: string;
    results: Api.Album[];
}>();

connection.listen((con, prev) => {
    document.body.dispatchEvent(new Event('connection-updated'));
    if (con?.id !== prev?.id) {
        document.body.dispatchEvent(new Event('connection-changed'));
    }
    if (!con) return;
    if (con.token !== prev?.token || con.clientId !== prev?.clientId) {
        api.refetchSignatureToken();
    }
});
onStartupFirst(async () => {
    const con = connection.get();
    if (con) {
        await refreshConnectionProfiles(con);
    }
});
onStartup(async () => {
    const con = connection.get();

    if (con && con.token && con.clientId) {
        try {
            await api.validateSignatureToken();
        } catch (e) {
            console.debug('Failed to validateSignatureToken:', e);
        }
    }
});
onStartup(async () => {
    const zones = await api.getAudioZones();

    setPlayerState(
        produce((state) => {
            state.audioZones = zones.items;

            const current = currentPlaybackTarget();

            if (current?.type === 'AUDIO_ZONE') {
                const existing = state.audioZones.find(
                    (x) => x.id === current.audioZoneId,
                );

                if (existing) {
                    state.currentAudioZone = existing;
                }
            }

            if (!state.currentAudioZone && !currentPlaybackTarget()) {
                state.currentAudioZone = state.audioZones[0];
                if (state.currentAudioZone) {
                    setCurrentPlaybackTarget({
                        type: 'AUDIO_ZONE',
                        audioZoneId: state.currentAudioZone.id,
                    });
                }
            }
        }),
    );
});
