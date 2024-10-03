import { isServer } from 'solid-js/web';
import { clientAtom } from './util';

export const navigationBarExpanded = clientAtom<boolean>(
    true,
    'navigationBarExpanded',
);
export const showPlaybackSessions = clientAtom(false);
export const showPlaybackQuality = clientAtom(false);

type StartupCallback = () => void | Promise<void>;

declare global {
    interface Window {
        startupCallbacks: StartupCallback[];
    }

    var startupCallbacks: StartupCallback[];
}

if (isServer) global.startupCallbacks = global.startupCallbacks ?? [];
else window.startupCallbacks = window.startupCallbacks ?? [];

const startupCallbacks: StartupCallback[] = isServer
    ? globalThis.startupCallbacks
    : window.startupCallbacks;
let startedUp = false;

export function onStartupFirst(func: StartupCallback) {
    if (startedUp) {
        func();
        return;
    }
    startupCallbacks.unshift(func);
}

export async function onStartup(func: StartupCallback) {
    if (startedUp) {
        try {
            await func();
        } catch (e) {
            console.error('Startup error:', e);
        }
        return;
    }
    startupCallbacks.push(func);
}

export async function triggerStartup() {
    if (startedUp) return;
    startedUp = true;

    for (const func of startupCallbacks) {
        try {
            await func();
        } catch (e) {
            console.error('Startup error:', e);
        }
    }
}
