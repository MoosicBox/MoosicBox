import {
    SwapStrategy,
    appendQueryParams,
    createEventDelegator,
    elementFetch,
    methods,
    onAttr,
    splitHtml,
    triggerHandlers,
} from './core';

const METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH'] as const;

function handleResponse(element: HTMLElement, text: string): boolean {
    const { html, style } = splitHtml(text);

    if (style && element.id) {
        triggerHandlers('swapStyle', {
            id: element.id,
            style,
        });
    }

    // Read WHERE to swap
    const targetAttr = element.getAttribute('hx-target');
    const target: string | HTMLElement = targetAttr || element;

    // Read HOW to swap (default: outerHTML)
    const swapAttr =
        element.getAttribute('hx-swap')?.toLowerCase() || 'outerhtml';
    const strategy = swapAttr as SwapStrategy;

    triggerHandlers('swapHtml', {
        target,
        html,
        strategy,
    });

    return true;
}

type Fragment = { selector: string; element: HTMLElement };
interface ParsedResponse {
    primary: string | null;
    fragments: Fragment[];
    deleteSelectors: string[];
}

function parseResponse(responseText: string, headers: Headers): ParsedResponse {
    const hasFragments = headers.get('X-HyperChad-Fragments') === 'true';
    const deleteSelectorsHeader = headers.get('X-HyperChad-Delete-Selectors');
    const deleteSelectors: string[] = deleteSelectorsHeader
        ? JSON.parse(deleteSelectorsHeader)
        : [];

    if (!hasFragments) {
        // Simple response - just primary content
        return {
            primary: responseText,
            fragments: [],
            deleteSelectors,
        };
    }

    // Split by fragment marker
    const contents = responseText.split('\n<!--hyperchad-fragment-->\n');

    let primary: string | null = null;

    if (contents[0].length > 0) {
        primary = contents[0];
    }

    // Parse fragment elements
    const fragments: Fragment[] = [];

    for (let i = 1; i < contents.length; i++) {
        const content = contents[i];
        const split = content.indexOf('\n');
        const selector = content.substring(0, split);
        const fragment = content.substring(split + 1);
        const temp = document.createElement('template');
        const { html, style } = splitHtml(fragment);

        temp.innerHTML = html;

        // Get all top-level elements with IDs
        for (const element of Array.from(temp.content.children)) {
            if (!(element instanceof HTMLElement)) continue;
            if (style) {
                triggerHandlers('swapStyle', { id: element.id, style });
            }
            fragments.push({ selector, element });
        }
    }

    return { primary, fragments, deleteSelectors };
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

    const responseText = await resp.text();
    const { primary, fragments, deleteSelectors } = parseResponse(
        responseText,
        resp.headers,
    );

    // Handle fragment swaps (by ID)
    for (const fragment of fragments) {
        const targets =
            fragment.selector === 'this'
                ? [element]
                : document.querySelectorAll(fragment.selector);

        for (const target of targets) {
            if (!(target instanceof HTMLElement)) continue;
            triggerHandlers('swapHtml', {
                target,
                html: fragment.element.outerHTML,
                strategy: 'this', // Always this for fragments
            });
        }
    }

    // Handle delete selectors
    for (let selector of deleteSelectors) {
        const child = selector.startsWith('> ');
        const target = child ? element : document;
        selector = child ? selector.substring(2) : selector;
        if (selector === '') element.remove();
        if (!selector) continue;

        target.querySelectorAll(selector).forEach((el) => el.remove());
    }

    // Handle primary swap (to triggering element)
    if (primary !== null) {
        handleResponse(element, primary);
    }
}

/**
 * This will mutate the options argument passed in
 */
export async function processRoute(
    element: HTMLElement,
    options: RequestInit = {},
): Promise<boolean> {
    const headers = new Headers(options.headers ?? {});
    if (!headers.has('hx-request')) {
        headers.set('hx-request', 'true');
    }
    options.headers = headers;

    for (const method of METHODS) {
        let route = element.getAttribute(`hx-${method}`);
        if (route) {
            options.method = method;

            // GET requests can't have body - convert FormData to query params
            if (method === 'GET' && options.body instanceof FormData) {
                route = appendQueryParams(route, options.body);
                delete options.body;
            }

            await handleHtmlResponse(
                element,
                elementFetch(route, options, element),
            );
            return true;
        }
    }

    return false;
}

onAttr('hx-trigger', ({ element, attr }) => {
    if (attr === 'load') {
        return processRoute(element);
    }
});

const supportedTags: Record<string, boolean> = {
    BUTTON: true,
    A: true,
} as const;

for (const method of methods) {
    createEventDelegator('click', `hx-${method}`, (element, route, e) => {
        if (!route) return;
        if (!supportedTags[element.tagName]) return;

        e.preventDefault();

        handleHtmlResponse(
            element,
            elementFetch(
                route,
                {
                    method,
                    headers: {
                        'hx-request': 'true',
                    },
                },
                element,
            ),
        );

        return false;
    });
}

// Handle change events for elements with hx-trigger="change"
// This enables <select>, <input>, and <textarea> elements to trigger HTTP requests on value change
createEventDelegator('change', 'hx-trigger', (element, triggerValue, _e) => {
    if (triggerValue !== 'change') return;

    const inputElement = element as
        | HTMLInputElement
        | HTMLSelectElement
        | HTMLTextAreaElement;
    const name = inputElement.name;
    const value = inputElement.value;

    for (const method of METHODS) {
        let route = element.getAttribute(`hx-${method.toLowerCase()}`);
        if (!route) continue;

        const options: RequestInit = {
            method,
            headers: { 'hx-request': 'true' },
        };

        if (method === 'GET') {
            // Append as query parameter for GET requests
            if (name) {
                route = appendQueryParams(route, { name, value });
            }
        } else {
            // Use FormData for non-GET requests (matches form.ts pattern)
            if (name) {
                const formData = new FormData();
                formData.append(name, value);
                options.body = formData;
            }
        }

        handleHtmlResponse(element, elementFetch(route, options, element));
        return;
    }
});
