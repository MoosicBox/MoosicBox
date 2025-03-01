import { onAttr, triggerHandlers } from './core';

const cache: { [url: string]: string } = {};
const pending: { [url: string]: Promise<string | void> } = {};

async function initiateFetchDocument(url: string): Promise<string | void> {
    try {
        const response = await fetch(url);
        const html = await response.text();
        cache[url] = html;
        delete pending[url];
        return html;
    } catch (e) {
        console.error('Failed to fetch document', url, e);
    }
}

function isSelfTarget(target: string | null): boolean {
    return target === null || target === 'self';
}

export function navigate(url: string) {
    const existing = cache[url];

    if (typeof existing === 'string') {
        triggerHandlers('swapDom', {
            html: existing,
            url,
        });
        return false;
    } else if (typeof cache[url] !== 'string' && !pending[url]) {
        pending[url] = initiateFetchDocument(url);
    }

    const request = pending[url];

    if (request) {
        console.debug('Awaiting pending request', url);
        request.then((html) => {
            if (typeof html === 'string') {
                triggerHandlers('swapDom', {
                    html,
                    url,
                });
                return;
            }
            console.debug('Invalid response', url, html);
        });
        return false;
    }

    console.debug('no document for anchor');
}

onAttr('href', ({ element, attr }) => {
    if (attr[0] !== '/') return; // Only handle links for this site
    if (!isSelfTarget(element.getAttribute('target'))) return; // Don't handle for new tab
    if (element.getAttribute('hx-preload') === 'false') return;

    element.onmouseenter = (_event) => {
        const existing = typeof cache[attr] === 'string' || pending[attr];

        if (!existing) {
            pending[attr] = initiateFetchDocument(attr);
        }
    };

    element.onclick = (event) => {
        if (event.ctrlKey) return; // Don't handle for new tab

        event.preventDefault();

        return navigate(attr);
    };
});
