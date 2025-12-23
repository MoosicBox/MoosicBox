import { triggerMessage, v } from './core';
import { fetchEventSource } from './vendored/fetch-event-source';

function getClientStreamId() {
    let id = localStorage.getItem('streamId');
    if (!id) {
        id = v.genUuid();
        localStorage.setItem('streamId', id);
    }
    return id;
}

export function initSSE() {
    const streamId = getClientStreamId();

    document.cookie = `v-sse-stream-id=${streamId}; path=/; SameSite=Strict`;

    fetchEventSource('/$sse', {
        method: 'GET',
        onopen: async (response: Response) => {
            if (response.status >= 400) {
                const status = response.status.toString();
                console.error('Failed to open SSE', { status });
            }
        },
        onmessage: (e) => triggerMessage(e.event, e.data, e.id),
        onerror: (error) => {
            if (error) {
                if (typeof error === 'object' && 'message' in error) {
                    console.error('SSE error', error.message);
                } else {
                    console.error('SSE error', error);
                }
            } else {
                console.error('SSE error', error);
            }
        },
    });
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initSSE);
} else {
    initSSE();
}
