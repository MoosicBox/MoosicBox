import { onAttr, triggerHandlers } from './core';

const METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH'] as const;

function handleResponse(element: HTMLElement, html: string): boolean {
    const swap = element.getAttribute('hx-swap');
    const swapLower = swap?.toLowerCase();

    switch (swapLower) {
        case 'outerhtml': {
            triggerHandlers('swap', { target: element, html, inner: false });
            return false;
        }
        case 'innerhtml': {
            triggerHandlers('swap', { target: element, html, inner: true });
            return false;
        }
        default: {
            if (swap) {
                const target = document.querySelector(swap) as HTMLElement;

                if (target) {
                    triggerHandlers('swap', { target, html, inner: false });
                }
            }
        }
    }

    return true;
}

async function handleHtmlResponse(
    element: HTMLElement,
    response: Promise<Response>,
): Promise<void> {
    handleResponse(element, await (await response).text());
}

function processRoute(element: HTMLElement): boolean {
    const options: RequestInit = {
        headers: {
            'hx-request': 'true',
        },
    };

    for (const method of METHODS) {
        const route = element.getAttribute(`hx-${method}`);
        if (route) {
            options.method = method;
            handleHtmlResponse(element, fetch(route, options));
        }
    }

    return true;
}

onAttr('hx-trigger', ({ element, attr }) => {
    if (attr === 'load') {
        return processRoute(element);
    }
});
