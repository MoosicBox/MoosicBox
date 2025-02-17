import { onAttr, triggerHandlers } from './core';

const cache: { [url: string]: string } = {};
const pending: { [url: string]: Promise<string | void> } = {};

async function initiateFetchDocument(url: string): Promise<string | void> {
    try {
        const response = await fetch(url);
        const html = await response.text();
        cache[url] = html;
        delete pending[url];
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

        function swap(html: string) {
            console.debug('Navigating to', attr);
            history.pushState({}, '', attr);
            document.documentElement.innerHTML = html;
            triggerHandlers('domLoad', {
                initial: true,
                navigation: false,
                element: document.documentElement,
            });
        }

        if (typeof existing === 'string') {
            swap(existing);
            return false;
        }

        const request = pending[attr];

        if (request) {
            request.then((html) => {
                if (typeof html === 'string') {
                    swap(html);
                }
            });
            return false;
        }

        console.debug('no document for anchor');
    });
});
