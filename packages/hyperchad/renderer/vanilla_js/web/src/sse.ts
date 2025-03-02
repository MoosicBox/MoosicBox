import { triggerMessage } from './core';
import { fetchEventSource } from './vendored/fetch-event-source';

fetchEventSource('$sse', {
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
