export const EVENT = {
    domLoaded: 'DOM_LOADED',
};

export type EventPayloads = {
    domLoaded: {
        initial: boolean;
        element: HTMLElement;
    };
};

export type EventType = keyof typeof EVENT;
export type Handler<T extends EventType> = (payload: EventPayloads[T]) => void;

type Handlers = { [K in EventType]: Handler<K>[] };

const handlers: Handlers = {} as Handlers;

export function on<T extends EventType>(event: T, handler: Handler<T>): void {
    let array = handlers[event];

    if (!array) {
        array = [];
        handlers[event] = array;
    }

    array.push(handler);
}

function triggerHandlers<T extends EventType>(
    event: T,
    payload: EventPayloads[T],
): void {
    handlers[event]?.forEach((handler) => {
        handler(payload);
    });
}

document.addEventListener('DOMContentLoaded', (event) => {
    const document = event.target as Document;
    const html = document.children[0] as HTMLHtmlElement;
    triggerHandlers('domLoaded', { initial: true, element: html });
});

function removeElementStyles(triggerId: string | undefined): void {
    if (triggerId) {
        document.querySelectorAll(`[v-id="${triggerId}"]`).forEach((style) => {
            style.remove();
        });
    }
}

function htmlToStyle(html: string, triggerId: string): HTMLStyleElement {
    const styleContainer = document.createElement('div');
    styleContainer.innerHTML = html;
    const style = styleContainer.children[0] as HTMLStyleElement;
    style.setAttribute('v-id', triggerId);
    return style;
}

function htmlToElement(
    html: string,
    triggerId: string | undefined,
): HTMLElement {
    const start = html.indexOf('\n\n');

    let elementText = html;

    if (start > 0) {
        elementText = html.substring(start + 2);

        if (triggerId) {
            const styleText = html.substring(0, start);
            document.head.appendChild(htmlToStyle(styleText, triggerId));
        }
    }

    const element = document.createElement('div');
    element.innerHTML = elementText;

    return element;
}

function processElement(element: HTMLElement) {
    triggerHandlers('domLoaded', { initial: false, element });
}

function processElementChildren(element: HTMLElement) {
    for (const child of element.children) {
        processElement(child as HTMLElement);
    }
}

function swapOuterHtml(element: HTMLElement, html: string) {
    const children = element.parentNode?.children;
    if (!children) return;

    removeElementStyles(element.id);

    const newElement = htmlToElement(html, element.id);

    const parent = element.parentNode;
    const child = newElement.lastChild;
    if (child) {
        element.replaceWith(newElement.removeChild(child));
    }

    while (newElement.children.length > 0) {
        parent.insertBefore(
            newElement.removeChild(newElement.lastChild!),
            child,
        );
    }

    processElementChildren(newElement);
}

function swapInnerHtml(element: HTMLElement, html: string) {
    const newElement = htmlToElement(html, element.id);
    element.innerHTML = newElement.innerHTML;
    processElementChildren(element);
}

function handleResponse(element: HTMLElement, html: string): boolean {
    const swap = element.getAttribute('hx-swap');
    const swapLower = swap?.toLowerCase();

    switch (swapLower) {
        case 'outerhtml': {
            swapOuterHtml(element, html);
            return false;
        }
        case 'innerhtml': {
            swapInnerHtml(element, html);
            return false;
        }
        default: {
            if (swap) {
                const target = document.querySelector(swap) as HTMLElement;

                if (target) {
                    swapOuterHtml(target, html);
                }
            }
        }
    }

    return true;
}

function handleHtmlResponse(
    element: HTMLElement,
    response: Promise<Response>,
): void {
    response
        .then((response) => {
            return response.text();
        })
        .then((html) => {
            handleResponse(element, html);
        });
}

const METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH'] as const;

function processRoute(element: HTMLElement): boolean {
    const options: RequestInit = {
        headers: {
            'hx-request': 'true',
        },
    };

    for (const method of METHODS) {
        const route = element.getAttribute(`hx-${method}`);
        if (route) {
            options.method = 'GET';
            handleHtmlResponse(element, fetch(route, options));
        }
    }

    return true;
}

function handleTrigger(trigger: string, element: HTMLElement): boolean {
    if (trigger === 'load') {
        return processRoute(element);
    }

    return true;
}

function checkTriggers(element: HTMLElement): void {
    const trigger = element.getAttribute('hx-trigger');

    if (trigger) {
        if (!handleTrigger(trigger, element)) {
            return;
        }
    }

    for (const child of element.children) {
        checkTriggers(child as HTMLElement);
    }
}

on('domLoaded', ({ element }) => {
    checkTriggers(element);
});
