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

type HttpTriggerAttr = (typeof HTTP_TRIGGER_ATTRS)[number];

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
    { startTime: number; url: string; method: string; controller?: AbortController }
>();

window.fetch = function (
    input: RequestInfo | URL,
    init?: RequestInit,
): Promise<Response> {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.toString() : input.url;
    const method = init?.method?.toUpperCase() ?? 'GET';
    const startTime = Date.now();

    const controller = init?.signal
        ? undefined
        : new AbortController();

    const detail: HttpEventDetail = {
        url,
        method,
    };

    dispatchHttpEvent('http-before-request', detail);

    let timeoutId: ReturnType<typeof setTimeout> | undefined;
    if (controller) {
        timeoutId = setTimeout(() => {
            controller.abort();
            dispatchHttpEvent('http-timeout', {
                ...detail,
                duration_ms: Date.now() - startTime,
                error: 'Request timeout',
            });
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

            dispatchHttpEvent('http-after-request', eventDetail);

            if (response.ok) {
                dispatchHttpEvent('http-success', eventDetail);
            } else {
                dispatchHttpEvent('http-error', {
                    ...eventDetail,
                    error: `HTTP ${response.status} ${response.statusText}`,
                });
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
                dispatchHttpEvent('http-abort', eventDetail);
            } else {
                dispatchHttpEvent('http-error', eventDetail);
            }

            dispatchHttpEvent('http-after-request', eventDetail);

            pendingRequests.delete(promise);
        });

    return promise;
};

function dispatchHttpEvent(eventType: string, detail: HttpEventDetail): void {
    const event = new CustomEvent(`hyperchad:${eventType}`, {
        detail,
        bubbles: true,
        cancelable: false,
    });
    document.dispatchEvent(event);
}

onElement(({ element }) => {
    for (const attr of HTTP_TRIGGER_ATTRS) {
        const action = element.getAttribute(attr);
        if (!action) continue;

        const eventType = attr.replace('v-', '');
        const decodedAction = decodeURIComponent(action);

        document.addEventListener(`hyperchad:${eventType}`, (event: Event) => {
            const customEvent = event as CustomEvent<HttpEventDetail>;
            const context = customEvent.detail;

            handleError(`http-event:${attr}`, () =>
                evaluate(decodedAction, {
                    element,
                    event,
                    value: context,
                }),
            );
        });
    }
});
