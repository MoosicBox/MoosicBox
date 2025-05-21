import { triggerMessage } from './core';

type EventName = string & Record<never, never>;

interface Event<T> {
    /** Event name */
    event: EventName;
    /** Event identifier used to unlisten */
    id: number;
    /** Event payload */
    payload: T;
}

declare global {
    interface Window {
        __TAURI__: {
            event: {
                listen: <T>(
                    event: string,
                    handler: (event: Event<T>) => void,
                ) => Promise<T>;
            };
        };
    }
}

interface TauriSseEvent {
    id?: string | undefined;
    event: string;
    data: string;
}

window.__TAURI__.event.listen<TauriSseEvent>('sse-event', (event) => {
    triggerMessage(event.payload.event, event.payload.data, event.payload.id);
});
