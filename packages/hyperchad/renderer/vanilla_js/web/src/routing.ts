import { methods, onAttr, onElement, splitHtml, triggerHandlers } from './core';

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

function raise(message: string): never {
    throw new Error(message);
}

async function handleHtmlResponse(
    element: HTMLElement,
    response: Promise<Response>,
): Promise<void> {
    const resp = await response;
    if (resp.status === 204) return;

    const contentType = resp.headers.get('content-type');
    if (!contentType || !contentType.includes('text/html')) {
        return;
    }

    let target = element;

    const fragment = resp.headers.get('v-fragment');

    if (fragment) {
        target =
            document.querySelector(fragment) ??
            raise(`Could not find element for fragment ${fragment}`);
    }

    handleResponse(target, await resp.text());
}

/**
 * This will mutate the options argument passed in
 */
export function processRoute(
    element: HTMLElement,
    options: RequestInit = {},
): boolean {
    const headers = new Headers(options.headers ?? {});
    if (!headers.has('hx-request')) {
        headers.set('hx-request', 'true');
    }
    options.headers = headers;

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

onElement(({ element }) => {
    if (!(element instanceof HTMLButtonElement)) return;

    let route: string | undefined = undefined;
    let method: string | undefined = undefined;

    for (const m of methods) {
        const r = element.getAttribute(`hx-${m}`);
        if (r) {
            route = r;
            method = m;
            break;
        }
    }

    if (!route) return;

    element.addEventListener('click', (e) => {
        e.preventDefault();

        handleHtmlResponse(
            element,
            fetch(route, {
                method,
                headers: {
                    'hx-request': 'true',
                },
            }),
        );

        return false;
    });
});
