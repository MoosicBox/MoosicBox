import { onAttr, splitHtml, triggerHandlers } from './core';

const METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH'] as const;

function handleResponse(element: HTMLElement, text: string): boolean {
    const { html, style } = splitHtml(text);
    const swap = element.getAttribute('hx-swap');
    const swapLower = swap?.toLowerCase();

    if (style && element.id) {
        triggerHandlers('swapStyle', {
            id: element.id,
            style,
        });
    }

    let inner = false;
    let target: string | HTMLElement = element;

    switch (swapLower) {
        case 'outerhtml':
            break;
        case 'innerhtml':
            inner = true;
            break;
        default:
            if (swap) target = swap;
    }

    triggerHandlers('swapHtml', {
        target,
        html,
        inner,
    });

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
