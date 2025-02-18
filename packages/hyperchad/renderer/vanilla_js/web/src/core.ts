export const EVENT = {
    domLoad: 'DOM_LOAD',
} as const;

export type EventPayloads = {
    domLoad: {
        initial: boolean;
        navigation: boolean;
        element: HTMLElement;
    };
    onAttr: {
        element: HTMLElement;
        attr: string;
    };
};

export type EventType = keyof typeof EVENT;
export type EventPayloadType = keyof EventPayloads;
export type Handler<T extends EventPayloadType> = (
    payload: EventPayloads[T],
) => void;

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

type AttrHandlers = { [attr: string]: Handler<'onAttr'>[] };

const attrHandlers: AttrHandlers = {} as AttrHandlers;

export function onAttr(attr: string, handler: Handler<'onAttr'>): void {
    let array = attrHandlers[attr];

    if (!array) {
        array = [];
        attrHandlers[attr] = array;
    }

    array.push(handler);
}

export function triggerHandlers<T extends EventType>(
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
    triggerHandlers('domLoad', {
        initial: true,
        navigation: false,
        element: html,
    });
});

function removeElementStyles(triggerId: string | undefined): void {
    if (triggerId) {
        document.querySelectorAll(`[v-id="${triggerId}"]`).forEach((style) => {
            style.remove();
        });
    }
}

export function htmlToStyle(html: string, triggerId: string): HTMLStyleElement {
    const styleContainer = document.createElement('div');
    styleContainer.innerHTML = html;
    const style = styleContainer.children[0] as HTMLStyleElement;
    style.setAttribute('v-id', triggerId);
    return style;
}

export function htmlToElement(
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

export function processElement(element: HTMLElement) {
    for (const key in attrHandlers) {
        const attr = element.getAttribute(key);
        if (attr) {
            attrHandlers[key].forEach((handler) => handler({ element, attr }));
        }
    }
    for (const child of element.children) {
        processElement(child as HTMLElement);
    }
}

export function swapOuterHtml(element: HTMLElement, html: string) {
    const children = element.parentNode?.children;
    if (!children) return;

    removeElementStyles(element.id);

    const newElement = htmlToElement(html, element.id);

    const parent = element.parentNode;
    const child = newElement.lastChild;
    const newChildren = [];
    if (child) {
        const newChild = newElement.removeChild(child) as HTMLElement;
        element.replaceWith(newChild);
        newChildren.push(newChild);
    }

    while (newElement.children.length > 0) {
        const newChild = newElement.removeChild(
            newElement.lastChild!,
        ) as HTMLElement;
        parent.insertBefore(newChild, child);
        newChildren.push(newChild);
    }

    for (const element of newChildren) {
        triggerHandlers('domLoad', {
            initial: false,
            navigation: false,
            element,
        });
    }
}

export function swapInnerHtml(element: HTMLElement, html: string) {
    const newElement = htmlToElement(html, element.id);
    element.innerHTML = newElement.innerHTML;
    for (const child of element.children) {
        triggerHandlers('domLoad', {
            initial: false,
            navigation: false,
            element: child as HTMLElement,
        });
    }
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

on('domLoad', ({ element }) => {
    processElement(element);
});
