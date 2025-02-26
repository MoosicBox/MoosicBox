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

export function htmlToStyle(
    html: string,
    triggerId: string,
): HTMLStyleElement | undefined {
    if (html === '<style></style>') return undefined;

    const styleContainer = document.createElement('div');
    styleContainer.innerHTML = html;
    const style = styleContainer.children[0] as HTMLStyleElement;
    style.setAttribute('v-id', triggerId);
    return style;
}

export function htmlToElement(html: string): HTMLElement {
    const start = html.indexOf('\n\n');

    let elementText = html;
    let styleText;

    if (start > 0) {
        elementText = html.substring(start + 2);
        styleText = html.substring(0, start);
    }

    const element = document.createElement('div');
    element.innerHTML = elementText;

    if (styleText && element.children.length === 1) {
        const triggerId = element.children[0].id;

        if (triggerId) {
            removeElementStyles(triggerId);
            const style = htmlToStyle(styleText, triggerId);
            if (style) {
                document.head.appendChild(style);
            }
        }
    }

    return element;
}

export function decodeHtml(html: string) {
    const txt = document.createElement('textarea');
    txt.innerHTML = html;
    return txt.value;
}

export function processElement(element: HTMLElement) {
    for (const key in attrHandlers) {
        const attr = element.getAttribute(key);
        if (attr) {
            attrHandlers[key].forEach((handler) =>
                handler({ element, attr: decodeHtml(attr) }),
            );
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

    const newElement = htmlToElement(html);

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
    const newElement = htmlToElement(html);
    element.innerHTML = newElement.innerHTML;
    for (const child of element.children) {
        triggerHandlers('domLoad', {
            initial: false,
            navigation: false,
            element: child as HTMLElement,
        });
    }
}

export function swapDom(html: string, url?: string | undefined) {
    if (typeof url === 'string') {
        console.debug('Navigating to', url);
        history.pushState({}, '', url);
    }
    document.documentElement.innerHTML = html;
    triggerHandlers('domLoad', {
        initial: true,
        navigation: false,
        element: document.documentElement,
    });
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

async function handleHtmlResponse(
    element: HTMLElement,
    response: Promise<Response>,
): Promise<void> {
    handleResponse(element, await (await response).text());
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

export function handleError<T>(type: string, func: () => T): T | undefined {
    try {
        return func();
    } catch (e) {
        console.error(`${type} failed`, e);
    }
}

onAttr('hx-trigger', ({ element, attr }) => {
    if (attr === 'load') {
        return processRoute(element);
    }
});

on('domLoad', ({ element }) => {
    processElement(element);
});
