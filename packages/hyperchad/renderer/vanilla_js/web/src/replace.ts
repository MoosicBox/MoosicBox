import { on, splitHtml, triggerHandlers, clearProcessedElements } from './core';

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
    const parent = element.parentElement;
    if (!parent) return;

    const newChildren: HTMLElement[] = [];

    if (typeof html === 'string') {
        const wrapper = htmlToElement(html);
        const replacements = Array.from(wrapper.children).filter(
            (child): child is HTMLElement => child instanceof HTMLElement,
        );

        if (replacements.length === 0) {
            element.remove();
            return;
        }

        const [first, ...rest] = replacements;
        element.replaceWith(first);
        newChildren.push(first);

        let previous = first;
        for (const replacement of rest) {
            previous.insertAdjacentElement('afterend', replacement);
            newChildren.push(replacement);
            previous = replacement;
        }
    } else {
        element.replaceWith(html);
        newChildren.push(html);
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

    // Clear processed elements cache for full document swaps
    clearProcessedElements();

    document.documentElement.innerHTML =
        typeof html === 'string' ? html : html.outerHTML;
    triggerHandlers('domLoad', {
        initial: true,
        navigation: false,
        elements: [document.documentElement],
    });
}

on('swapDom', ({ html, url }) => swapDom(html, url));
on('swapHtml', ({ target, html, strategy }) => {
    // For replace.ts, we only support children and this
    // Other strategies are handled by idiomorph.ts
    if (strategy === 'children') {
        swapInnerHtml(target, html);
    } else {
        swapOuterHtml(target, html);
    }
});
