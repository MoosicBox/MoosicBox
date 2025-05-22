import { triggerHandlers, onAttr } from './core';

export const cache: { [url: string]: string } = {};
export const pending: { [url: string]: Promise<string | void> } = {};

export function isSelfTarget(target: string | null): boolean {
    return target === null || target === 'self';
}

export function handleNavigation(url: string, html: string) {
    triggerHandlers('swapDom', {
        html,
        url,
    });
}

export function handleLinkClick(event: MouseEvent, href: string | null) {
    if (!href || href[0] !== '/') return false; // Only handle links for this site
    const link = (event.target as HTMLElement).closest('a');
    if (!link) return false;
    if (!isSelfTarget(link.getAttribute('target'))) return false; // Don't handle for new tab
    if (link.getAttribute('hx-preload') === 'false') return false;
    if (event.ctrlKey) return false; // Don't handle for new tab

    event.preventDefault();
    return true;
}

export function handleLinkHover(
    event: MouseEvent,
    href: string | null,
    initiateFetch: (url: string) => Promise<string | void>,
) {
    if (!href || href[0] !== '/') return; // Only handle links for this site
    const link = (event.target as HTMLElement).closest('a');
    if (!link) return;
    if (!isSelfTarget(link.getAttribute('target'))) return; // Don't handle for new tab
    if (link.getAttribute('hx-preload') === 'false') return;

    const existing = typeof cache[href] === 'string' || pending[href];
    if (!existing) {
        pending[href] = initiateFetch(href);
    }
}

export function setupLinkHandlers(
    initiateFetch: (url: string) => Promise<string | void>,
    navigate: (url: string) => void,
) {
    onAttr('href', ({ element, attr }) => {
        if (attr[0] !== '/') return; // Only handle links for this site
        if (!isSelfTarget(element.getAttribute('target'))) return; // Don't handle for new tab
        if (element.getAttribute('hx-preload') === 'false') return;

        element.onmouseenter = (_event) => {
            const existing = typeof cache[attr] === 'string' || pending[attr];
            if (!existing) {
                pending[attr] = initiateFetch(attr);
            }
        };

        element.onclick = (event) => {
            if (event.ctrlKey) return; // Don't handle for new tab
            event.preventDefault();
            navigate(attr);
        };
    });
}
