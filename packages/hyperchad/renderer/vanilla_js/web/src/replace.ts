import { on, splitHtml, triggerHandlers } from './core';

export function htmlToElement(html: string): HTMLElement {
    const { html: elementText, style } = splitHtml(html);

    const element = document.createElement('div');
    element.innerHTML = elementText;

    if (style && element.children.length === 1) {
        const triggerId = element.children[0].id;

        if (triggerId) {
            triggerHandlers('swapStyle', { style, id: triggerId });
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

    triggerHandlers('domLoad', {
        initial: false,
        navigation: false,
        elements: newChildren,
    });
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
    triggerHandlers('domLoad', {
        initial: false,
        navigation: false,
        elements: Array.from(element.children) as HTMLElement[],
    });
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
        elements: [document.documentElement],
    });
}

on('swapDom', ({ html, url }) => swapDom(html, url));
on('swapHtml', ({ target, html, inner }) =>
    inner ? swapInnerHtml(target, html) : swapOuterHtml(target, html),
);
