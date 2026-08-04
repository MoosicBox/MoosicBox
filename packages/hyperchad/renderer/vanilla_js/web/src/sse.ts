import { on, triggerMessage } from './core';
import { startEventSourceStream, stopEventSourceStream } from './sse-base';

const DEFAULT_SSE_STREAM_KEY = '/$sse';

export {
    clearClientStreamId,
    createEventSourcePath,
    DEFAULT_SSE_STREAM_ID_COOKIE_NAME,
    DEFAULT_SSE_STREAM_ID_STORAGE_KEY,
    getOrCreateClientStreamId,
    hasActiveEventSourceStream,
    setStreamIdCookie,
    stopAllEventSourceStreams,
    stopEventSourceStream,
    startEventSourceStream,
    type EventSourceStreamOptions,
} from './sse-base';

export function initSSE() {
    const query = new URLSearchParams(window.location.search);
    const eventScope = query.get('hyperchad-event-scope');
    const path = eventScope
        ? `/$sse?hyperchad-event-scope=${encodeURIComponent(eventScope)}`
        : '/$sse';
    startEventSourceStream(path, {
        streamKey: DEFAULT_SSE_STREAM_KEY,
        onmessage: (e) => triggerMessage(e.event, e.data, e.id),
    });
}

export function stopSSE() {
    stopEventSourceStream(DEFAULT_SSE_STREAM_KEY);
}

on('domLoad', ({ navigation }) => {
    if (navigation) {
        stopSSE();
        initSSE();
    }
});

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initSSE);
} else {
    initSSE();
}
