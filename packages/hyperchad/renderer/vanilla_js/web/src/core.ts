export const EVENT = {
    domLoad: 'DOM_LOAD',
    swapDom: 'SWAP_DOM',
    swapHtml: 'SWAP_HTML',
    swapStyle: 'SWAP_STYLE',
} as const;

export type EventPayloads = {
    domLoad: {
        initial: boolean;
        navigation: boolean;
        elements: HTMLElement[];
    };
    swapDom: {
        html: string | HTMLElement;
        url?: string | undefined;
    };
    swapHtml: {
        target: string | HTMLElement;
        html: string | HTMLElement;
        inner: boolean;
    };
    swapStyle: {
        id: string;
        style: string | HTMLElement;
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

export type MessageHandler = (data: string, id?: string | undefined) => void;
type MessageHandlers = { [type: string]: MessageHandler[] };

const messageHandlers: MessageHandlers = {} as MessageHandlers;

export function onMessage(type: string, handler: MessageHandler): void {
    let array = messageHandlers[type];

    if (!array) {
        array = [];
        messageHandlers[type] = array;
    }

    array.push(handler);
}

export function triggerMessage(type: string, data: string): void {
    messageHandlers[type]?.forEach((handler) => {
        handler(data);
    });
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
        elements: [html],
    });
});

export function removeElementStyles(triggerId: string | undefined): void {
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
    const styleContainer = document.createElement('div');
    styleContainer.innerHTML = html;
    const style = styleContainer.children[0] as HTMLStyleElement;
    style.setAttribute('v-id', triggerId);
    return style;
}

export function splitHtml(html: string): {
    html: string;
    style?: string | undefined;
} {
    const start = html.indexOf('\n\n');

    if (start > 0) {
        return {
            html: html.substring(start + 2),
            style: html.substring(0, start),
        };
    }

    return { html };
}

export function decodeHtml(html: string) {
    const txt = document.createElement('textarea');
    txt.innerHTML = html;
    return txt.value;
}

export function processElement(element: HTMLElement) {
    for (const key in attrHandlers) {
        const attr = element.getAttribute(key);

        if (!attr) continue;

        attrHandlers[key].forEach((handler) =>
            handler({ element, attr: decodeHtml(attr) }),
        );
    }
    for (const child of element.children) {
        processElement(child as HTMLElement);
    }
}

export function handleError<T>(type: string, func: () => T): T | undefined {
    try {
        return func();
    } catch (e) {
        console.error(`${type} failed`, e);
    }
}

on('domLoad', ({ elements }) => elements.forEach(processElement));
on('swapStyle', ({ id, style }) => {
    removeElementStyles(id);

    if (!style) return;

    const styleElement =
        typeof style === 'string' ? htmlToStyle(style, id) : style;

    if (!styleElement) return;

    document.head.appendChild(styleElement);
});
onMessage('view', (html) => triggerHandlers('swapDom', { html }));
onMessage('partial_view', (data, id) => {
    if (!id) return;

    const { html, style } = splitHtml(data);

    if (style && style !== '<style></style>') {
        triggerHandlers('swapStyle', { style, id });
    }

    triggerHandlers('swapHtml', { html, inner: false, target: id });
});
