import { onAttr, swapDom } from './core';

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

onAttr('href', ({ element, attr }) => {
    if (attr[0] !== '/') return; // Only handle links for this site
    if (!isSelfTarget(element.getAttribute('target'))) return; // Don't handle for new tab
    if (element.getAttribute('hx-preload') === 'false') return;

    element.addEventListener('mouseenter', (_event) => {
        const existing = typeof cache[attr] === 'string' || pending[attr];

        if (!existing) {
            pending[attr] = initiateFetchDocument(attr);
        }
    });

    element.addEventListener('click', (event) => {
        if (event.ctrlKey) return; // Don't handle for new tab

        event.preventDefault();

        const existing = cache[attr];

        if (typeof existing === 'string') {
            swapDom(existing, attr);
            return false;
        }

        const request = pending[attr];

        if (request) {
            console.debug('Awaiting pending request', attr);
            request.then((html) => {
                if (typeof html === 'string') {
                    swapDom(html, attr);
                    return;
                }
                console.debug('Invalid response', attr, html);
            });
            return false;
        }

        console.debug('no document for anchor');
    });
});
