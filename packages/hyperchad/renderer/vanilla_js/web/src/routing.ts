import {
    SwapStrategy,
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

function raise(message: string): never {
    throw new Error(message);
}

interface ParsedResponse {
    primary: string | null;
    fragments: HTMLElement[];
}

function parseResponse(responseText: string, headers: Headers): ParsedResponse {
    const hasFragments = headers.get('X-HyperChad-Fragments') === 'true';

    if (!hasFragments) {
        // Simple response - just primary content
        return {
            primary: responseText,
            fragments: [],
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
        const temp = document.createElement('div');
        temp.innerHTML = fragmentsHtml;

        // Get all top-level elements with IDs
        for (const child of Array.from(temp.children)) {
            if (child instanceof HTMLElement && child.id) {
                fragments.push(child);
            } else if (child instanceof HTMLElement) {
                console.warn('Fragment element missing ID attribute:', child);
            }
        }
    }

    return { primary, fragments };
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
    const { primary, fragments } = parseResponse(responseText, resp.headers);

    // Handle primary swap (to triggering element)
    if (primary !== null) {
        handleResponse(element, primary);
    }

    // Handle fragment swaps (by ID)
    for (const fragment of fragments) {
        const targetId = fragment.id;
        const target = document.getElementById(targetId);

        if (!target) {
            console.warn(`Fragment target not found: #${targetId}`);
            continue;
        }

        const { html, style } = splitHtml(fragment.outerHTML);

        if (style && targetId) {
            triggerHandlers('swapStyle', { id: targetId, style });
        }

        triggerHandlers('swapHtml', {
            target,
            html,
            strategy: 'this', // Always this for fragments
        });
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
