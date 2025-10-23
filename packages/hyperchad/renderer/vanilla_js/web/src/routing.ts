import {
    SwapStrategy,
    elementFetch,
    methods,
    onAttr,
    onElement,
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

interface ParsedResponse {
    primary: string | null;
    fragments: HTMLElement[];
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
    const fragmentStart = responseText.indexOf('<!--hyperchad-fragments-->');
    const fragmentEnd = responseText.indexOf('<!--hyperchad-fragments-end-->');

    let primary: string | null = null;
    let fragmentsHtml = '';

    if (fragmentStart > 0) {
        primary = responseText.substring(0, fragmentStart).trim();
    }

    if (fragmentStart >= 0 && fragmentEnd > fragmentStart) {
        fragmentsHtml = responseText
            .substring(
                fragmentStart + '<!--hyperchad-fragments-->'.length,
                fragmentEnd,
            )
            .trim();
    }

    // Parse fragment elements
    const fragments: HTMLElement[] = [];
    if (fragmentsHtml) {
        const temp = document.createElement('template');
        const { html, style } = splitHtml(fragmentsHtml);

        temp.innerHTML = html;

        // Get all top-level elements with IDs
        for (const child of Array.from(temp.content.children)) {
            if (!(child instanceof HTMLElement)) continue;
            if (!child.id) {
                console.warn('Fragment element missing ID attribute:', child);
                continue;
            }

            if (style) {
                triggerHandlers('swapStyle', { id: child.id, style });
            }
            fragments.push(child);
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

    // Handle primary swap (to triggering element)
    if (primary !== null) {
        handleResponse(element, primary);
    }

    // Handle fragment swaps (by ID)
    for (const fragment of fragments) {
        const targetId = fragment.id;
        const target = document.getElementById(targetId);
        if (!target) continue;

        triggerHandlers('swapHtml', {
            target,
            html: fragment.outerHTML,
            strategy: 'this', // Always this for fragments
        });
    }

    // Handle delete selectors
    for (const selector of deleteSelectors) {
        if (selector === '') element.remove();
        if (!selector) continue;

        document.querySelectorAll(selector).forEach((el) => el.remove());
    }
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
            handleHtmlResponse(element, elementFetch(route, options, element));
        }
    }

    return true;
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

onElement(({ element }) => {
    if (!supportedTags[element.tagName]) return;

    for (const method of methods) {
        const route = element.getAttribute(`hx-${method}`);
        if (!route) continue;

        element.addEventListener('click', (e) => {
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
});
