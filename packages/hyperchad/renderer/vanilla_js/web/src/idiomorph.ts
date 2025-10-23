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
on('swapHtml', ({ target, html, strategy }) => {
    // Resolve target to element
    if (typeof target === 'string') {
        const element = document.querySelector(target);
        if (!element) {
            console.warn(`Swap target not found: ${target}`);
            return;
        }
        target = element as HTMLElement;
    }

    const addedElements: HTMLElement[] = [];

    // Handle delete
    if (strategy === 'delete') {
        target.remove();
        return;
    }

    // Handle none
    if (strategy === 'none') {
        return;
    }

    // Handle positional insertions using native DOM API
    if (
        ['beforebegin', 'afterbegin', 'beforeend', 'afterend'].includes(
            strategy,
        )
    ) {
        const htmlString = typeof html === 'string' ? html : html.outerHTML;
        const position = strategy as InsertPosition;

        target.insertAdjacentHTML(position, htmlString);

        // Collect newly inserted elements
        const newElements = getInsertedElements(target, position);
        addedElements.push(...newElements);

        if (addedElements.length > 0) {
            triggerHandlers('domLoad', {
                initial: false,
                navigation: false,
                elements: addedElements,
            });
        }
        return;
    }

    // Handle morph strategies (children, this) using Idiomorph
    // Map to idiomorph's innerHTML/outerHTML terminology
    Idiomorph.morph(target, html, {
        morphStyle: strategy === 'children' ? 'innerHTML' : 'outerHTML',
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
                    // Could track morphed elements if needed
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

function getInsertedElements(
    target: HTMLElement,
    position: InsertPosition,
): HTMLElement[] {
    const elements: HTMLElement[] = [];

    switch (position) {
        case 'beforebegin':
            if (target.previousElementSibling instanceof HTMLElement) {
                elements.push(target.previousElementSibling);
            }
            break;
        case 'afterbegin':
            if (target.firstElementChild instanceof HTMLElement) {
                elements.push(target.firstElementChild);
            }
            break;
        case 'beforeend':
            if (target.lastElementChild instanceof HTMLElement) {
                elements.push(target.lastElementChild);
            }
            break;
        case 'afterend':
            if (target.nextElementSibling instanceof HTMLElement) {
                elements.push(target.nextElementSibling);
            }
            break;
    }

    return elements;
}
