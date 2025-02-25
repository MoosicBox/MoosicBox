import { htmlToElement, triggerHandlers } from './core';
import { fetchEventSource } from './fetch-event-source';

fetchEventSource('$sse', {
    method: 'GET',
    onopen: async (response: Response) => {
        if (response.status >= 400) {
            const status = response.status.toString();
            console.error('Failed to open SSE', { status });
        }
    },
    onmessage: (e) => {
        console.log('SSE event', e);

        switch (e.event) {
            case 'partial_view': {
                const element = htmlToElement(e.data);
                if (element.children.length === 1) {
                    const target = document.getElementById(
                        element.children[0].id,
                    );
                    if (target) {
                        target.replaceWith(element);
                        triggerHandlers('domLoad', {
                            element,
                            initial: false,
                            navigation: false,
                        });
                    }
                }
                break;
            }
        }
    },
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
