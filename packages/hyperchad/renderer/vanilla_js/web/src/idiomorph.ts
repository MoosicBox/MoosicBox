import { on, triggerHandlers } from './core';
import { Idiomorph } from './vendored/idiomorph.esm';

on('swapDom', ({ html, url }) => {
    if (typeof url === 'string') {
        console.debug('Navigating to', url);
        history.pushState({}, '', url);
    }
    if (typeof html === 'string' && html.indexOf('<!DOCTYPE') === 0) {
        html = html.substring(html.indexOf('>') + 1);
    }
    Idiomorph.morph(document.documentElement, html, {
        head: { style: 'morph' },
    });
    triggerHandlers('domLoad', {
        initial: false,
        navigation: typeof url === 'string',
        elements: [document.documentElement],
    });
});
on('swapHtml', ({ target, html, inner }) => {
    if (typeof target === 'string') {
        const element = document.querySelector(target);
        if (!element) return;
        target = element as HTMLElement;
    }
    const elements: HTMLElement[] = [];
    const returned: HTMLElement[] = Idiomorph.morph(target, html, {
        morphStyle: inner ? 'innerHTML' : 'outerHTML',
        callbacks: {
            afterNodeAdded(node: Node) {
                if (node instanceof HTMLElement) {
                    elements.push(node);
                }
            },
            afterNodeMorphed(_old: Node, node: Node) {
                if (node instanceof HTMLElement) {
                    elements.push(node);
                }
            },
        },
    }).filter((x) => x instanceof HTMLElement) as HTMLElement[];

    if (elements.length > 0) {
        if (returned.length > 0) {
            elements.push(...returned);
        }
        triggerHandlers('domLoad', {
            initial: false,
            navigation: false,
            elements,
        });
    }
});
