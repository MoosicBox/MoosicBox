import { Api, api, connection, refreshConnectionProfiles } from './api';
import { createSignal } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import { clientAtom } from './util';
import {
    currentPlaybackTarget,
    setCurrentPlaybackTarget,
    setPlayerState,
} from './player';

export const navigationBarExpanded = clientAtom<boolean>(
    true,
    'navigationBarExpanded',
);
export const showAudioZones = clientAtom(false);
export const showPlaybackSessions = clientAtom(false);
export const showPlaybackQuality = clientAtom(false);
export const showChangePlaybackTargetModal = clientAtom(false);
export const showScanStatusBanner = clientAtom(false);

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

export function onStartupFirst(func: StartupCallback) {
    if (isStartedUp()) {
        func();
        return;
    }
    getStartupCallbacks().unshift(func);
}

export async function onStartup(func: StartupCallback) {
    if (isStartedUp()) {
        try {
            await func();
        } catch (e) {
            console.error('Startup error:', e);
        }
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
onStartupFirst(() => {
    const con = connection.get();
    if (con) {
        refreshConnectionProfiles(con);
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
