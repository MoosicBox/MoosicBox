import { on, onMessage, triggerHandlers } from './core';

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

export function swapOuterHtml(
    element: string | HTMLElement,
    html: string | HTMLElement,
) {
    if (typeof element === 'string') {
        const target = document.querySelector(element);
        if (!target) return;
        element = target as HTMLElement;
    }
    const children = element.parentNode?.children;
    if (!children) return;

    removeElementStyles(element.id);

    const newElement = typeof html === 'string' ? htmlToElement(html) : html;

    const parent = element.parentNode!;
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

export function swapInnerHtml(
    element: string | HTMLElement,
    html: string | HTMLElement,
) {
    if (typeof element === 'string') {
        const target = document.querySelector(element);
        if (!target) return;
        element = target as HTMLElement;
    }
    const newElement = typeof html === 'string' ? htmlToElement(html) : html;
    element.innerHTML = newElement.innerHTML;
    for (const child of element.children) {
        triggerHandlers('domLoad', {
            initial: false,
            navigation: false,
            element: child as HTMLElement,
        });
    }
}

export function swapDom(html: string | HTMLElement, url?: string | undefined) {
    if (typeof url === 'string') {
        console.debug('Navigating to', url);
        history.pushState({}, '', url);
    }
    document.documentElement.innerHTML =
        typeof html === 'string' ? html : html.outerHTML;
    triggerHandlers('domLoad', {
        initial: true,
        navigation: false,
        element: document.documentElement,
    });
}

onMessage('view', swapDom);
onMessage('partial_view', (data) => {
    const element = htmlToElement(data);
    if (element.children.length === 1) {
        const replacement = element.children[0] as HTMLElement;
        const target = document.getElementById(element.children[0].id);
        if (target) {
            target.replaceWith(replacement);
            triggerHandlers('domLoad', {
                element: replacement,
                initial: false,
                navigation: false,
            });
        }
    }
});

on('swapDom', ({ html, url }) => swapDom(html, url));
on('swap', ({ target, html, inner }) =>
    inner ? swapInnerHtml(target, html) : swapOuterHtml(target, html),
);
