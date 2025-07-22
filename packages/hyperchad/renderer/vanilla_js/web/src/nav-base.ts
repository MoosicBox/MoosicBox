import { triggerHandlers } from './core';

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
    // Use global event delegation for link handling
    document.addEventListener(
        'mouseenter',
        (event) => {
            const target = event.target;
            if (!target || !(target instanceof HTMLElement)) return;

            const link = target.closest('a');
            if (!link) return;

            const href = link.getAttribute('href');
            if (!href || href[0] !== '/') return; // Only handle links for this site
            if (!isSelfTarget(link.getAttribute('target'))) return; // Don't handle for new tab
            if (link.getAttribute('hx-preload') === 'false') return;

            const existing = typeof cache[href] === 'string' || pending[href];
            if (!existing) {
                pending[href] = initiateFetch(href);
            }
        },
        true,
    );

    document.addEventListener(
        'click',
        (event) => {
            const target = event.target;
            if (!target || !(target instanceof HTMLElement)) return;

            const link = target.closest('a');
            if (!link) return;

            const href = link.getAttribute('href');
            if (!href || href[0] !== '/') return; // Only handle links for this site
            if (!isSelfTarget(link.getAttribute('target'))) return; // Don't handle for new tab
            if (link.getAttribute('hx-preload') === 'false') return;
            if (event.ctrlKey) return; // Don't handle for new tab

            event.preventDefault();
            navigate(href);
        },
        true,
    );
}
