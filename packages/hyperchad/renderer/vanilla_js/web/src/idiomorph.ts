import { on, triggerHandlers, clearProcessedElements } from './core';
import { Idiomorph } from './vendored/idiomorph.esm';

on('swapDom', ({ html, url }) => {
    if (typeof url === 'string') {
        console.debug('Navigating to', url);
        history.pushState({}, '', url);
    }
    if (typeof html === 'string' && html.indexOf('<!DOCTYPE') === 0) {
        html = html.substring(html.indexOf('>') + 1);
    }

    // Clear processed elements cache for full document swaps
    // This ensures elements get reprocessed even when returning to cached pages
    clearProcessedElements();

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

    const addedElements: HTMLElement[] = [];
    const morphedElements: HTMLElement[] = [];

    Idiomorph.morph(target, html, {
        morphStyle: inner ? 'innerHTML' : 'outerHTML',
        callbacks: {
            afterNodeAdded(node: Node) {
                if (node instanceof HTMLElement) {
                    addedElements.push(node);
                }
            },
            afterNodeMorphed(oldNode: Node, newNode: Node) {
                if (
                    oldNode instanceof HTMLElement &&
                    newNode instanceof HTMLElement
                ) {
                    morphedElements.push(oldNode);
                }
            },
        },
    });

    if (addedElements.length > 0) {
        triggerHandlers('domLoad', {
            initial: false,
            navigation: false,
            elements: addedElements,
        });
    }
});
