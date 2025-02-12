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

function textToStyle(text: string, triggerId: string): HTMLStyleElement {
    const styleContainer = document.createElement('div');
    styleContainer.innerHTML = text;
    const style = styleContainer.children[0] as HTMLStyleElement;
    style.setAttribute('v-id', triggerId);
    return style;
}

function textToElement(
    text: string,
    triggerId: string | undefined,
): HTMLElement {
    const start = text.indexOf('\n\n');

    let elementText = text;

    if (start > 0) {
        elementText = text.substring(start + 2);

        if (triggerId) {
            const styleText = text.substring(0, start);
            document.head.appendChild(textToStyle(styleText, triggerId));
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

function swapOuterHtml(element: HTMLElement, text: string) {
    const children = element.parentNode?.children;
    if (!children) return;

    removeElementStyles(element.id);

    const newElement = textToElement(text, element.id);

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

function swapInnerHtml(element: HTMLElement, text: string) {
    const newElement = textToElement(text, element.id);
    element.innerHTML = newElement.innerHTML;
    processElementChildren(element);
}

function handleResponse(element: HTMLElement, text: string): boolean {
    const swap = element.getAttribute('hx-swap');
    const swapLower = swap?.toLowerCase();

    switch (swapLower) {
        case 'outerhtml': {
            swapOuterHtml(element, text);
            return false;
        }
        case 'innerhtml': {
            swapInnerHtml(element, text);
            return false;
        }
        default: {
            if (swap) {
                const target = document.querySelector(swap) as HTMLElement;

                if (target) {
                    swapOuterHtml(target, text);
                }
            }
        }
    }

    return true;
}

function processRoute(element: HTMLElement): boolean {
    const getRoute = element.getAttribute('hx-get');
    const postRoute = element.getAttribute('hx-post');
    const putRoute = element.getAttribute('hx-put');
    const deleteRoute = element.getAttribute('hx-delete');
    const patchRoute = element.getAttribute('hx-patch');

    const options: RequestInit = {
        headers: {
            'hx-request': 'true',
        },
    };

    if (typeof getRoute === 'string') {
        options.method = 'GET';
        fetch(getRoute, options)
            .then((response) => {
                return response.text();
            })
            .then((text) => {
                handleResponse(element, text);
            });
    }
    if (typeof postRoute === 'string') {
        options.method = 'POST';
        fetch(postRoute, options)
            .then((response) => {
                return response.text();
            })
            .then((text) => {
                handleResponse(element, text);
            });
    }
    if (typeof putRoute === 'string') {
        options.method = 'PUT';
        fetch(putRoute, options)
            .then((response) => {
                return response.text();
            })
            .then((text) => {
                handleResponse(element, text);
            });
    }
    if (typeof deleteRoute === 'string') {
        options.method = 'DELETE';
        fetch(deleteRoute, options)
            .then((response) => {
                return response.text();
            })
            .then((text) => {
                handleResponse(element, text);
            });
    }
    if (typeof patchRoute === 'string') {
        options.method = 'PATCH';
        fetch(patchRoute, options)
            .then((response) => {
                return response.text();
            })
            .then((text) => {
                handleResponse(element, text);
            });
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
