import { evaluate } from './actions';
import { handleError, onElement } from './core';

const HTTP_TRIGGER_ATTRS = [
    'v-http-before-request',
    'v-http-after-request',
    'v-http-success',
    'v-http-error',
    'v-http-abort',
    'v-http-timeout',
] as const;

interface HttpEventDetail {
    url: string;
    method: string;
    status?: number;
    headers?: Record<string, string>;
    duration_ms?: number;
    error?: string;
}

const originalFetch = window.fetch;
const pendingRequests = new Map<
    Promise<Response>,
    {
        startTime: number;
        url: string;
        method: string;
        controller?: AbortController;
    }
>();

window.fetch = async function (
    input: RequestInfo | URL,
    init?: RequestInit,
    element?: HTMLElement,
): Promise<Response> {
    const url =
        typeof input === 'string'
            ? input
            : input instanceof URL
              ? input.toString()
              : input.url;

    const method = init?.method?.toUpperCase() ?? 'GET';
    const startTime = Date.now();

    const controller = init?.signal ? undefined : new AbortController();

    const detail: HttpEventDetail = {
        url,
        method,
    };

    dispatchHttpEvent('http-before-request', detail, element);

    let timeoutId: ReturnType<typeof setTimeout> | undefined;
    if (controller) {
        timeoutId = setTimeout(() => {
            controller.abort();
            dispatchHttpEvent(
                'http-timeout',
                {
                    ...detail,
                    duration_ms: Date.now() - startTime,
                    error: 'Request timeout',
                },
                element,
            );
        }, 30000);
    }

    const fetchInit = controller
        ? { ...init, signal: controller.signal }
        : init;

    const promise = originalFetch.call(window, input, fetchInit);

    pendingRequests.set(promise, { startTime, url, method, controller });

    promise
        .then((response) => {
            if (timeoutId) clearTimeout(timeoutId);

            const duration_ms = Date.now() - startTime;
            const headers: Record<string, string> = {};
            response.headers.forEach((value, key) => {
                headers[key] = value;
            });

            const eventDetail: HttpEventDetail = {
                url,
                method,
                status: response.status,
                headers,
                duration_ms,
            };

            dispatchHttpEvent('http-after-request', eventDetail, element);

            if (response.ok) {
                dispatchHttpEvent('http-success', eventDetail, element);
            } else {
                dispatchHttpEvent(
                    'http-error',
                    {
                        ...eventDetail,
                        error: `HTTP ${response.status} ${response.statusText}`,
                    },
                    element,
                );
            }

            pendingRequests.delete(promise);
        })
        .catch((error) => {
            if (timeoutId) clearTimeout(timeoutId);

            const duration_ms = Date.now() - startTime;
            const eventDetail: HttpEventDetail = {
                url,
                method,
                duration_ms,
                error: error.message ?? String(error),
            };

            if (error.name === 'AbortError') {
                dispatchHttpEvent('http-abort', eventDetail, element);
            } else {
                dispatchHttpEvent('http-error', eventDetail, element);
            }

            dispatchHttpEvent('http-after-request', eventDetail, element);

            pendingRequests.delete(promise);
        });

    return promise;
};

function dispatchHttpEvent(
    eventType: string,
    detail: HttpEventDetail,
    element?: HTMLElement,
): void {
    const event = new CustomEvent(`hyperchad:${eventType}`, {
        detail,
        bubbles: true,
        cancelable: false,
    });

    if (element) {
        element.dispatchEvent(event);
    } else {
        document.dispatchEvent(event);
    }
}

for (const attr of HTTP_TRIGGER_ATTRS) {
    const eventType = attr.replace('v-', '');
    document.addEventListener(`hyperchad:${eventType}`, (event: Event) => {
        const element = event.target as HTMLElement | undefined;
        if (!element) return;
        const customEvent = event as CustomEvent<HttpEventDetail>;
        const context = customEvent.detail;

        const handler = (element: Element) => {
            if (!(element instanceof HTMLElement)) return;
            const action = element.getAttribute(attr);
            if (!action) return;
            const decodedAction = decodeURIComponent(action);

            handleError(`http-event:${attr}`, () =>
                evaluate(decodedAction, {
                    element,
                    event,
                    value: context,
                }),
            );
        };

        handler(element);
        element.querySelectorAll(`[${attr}]`).forEach(handler);
    });
}
