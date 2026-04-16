import { triggerMessage } from './core';
import { startEventSourceStream, stopEventSourceStream } from './sse-base';

const DEFAULT_SSE_STREAM_KEY = '/$sse';

export {
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
    startEventSourceStream('/$sse', {
        streamKey: DEFAULT_SSE_STREAM_KEY,
        onmessage: (e) => triggerMessage(e.event, e.data, e.id),
    });
}

export function stopSSE() {
    stopEventSourceStream(DEFAULT_SSE_STREAM_KEY);
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initSSE);
} else {
    initSSE();
}
