import { cache, pending, handleNavigation, setupLinkHandlers } from './nav-base';

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

export function navigate(url: string) {
    const existing = cache[url];

    if (typeof existing === 'string') {
        handleNavigation(url, existing);
        return false;
    } else if (typeof cache[url] !== 'string' && !pending[url]) {
        pending[url] = initiateFetchDocument(url);
    }

    const request = pending[url];

    if (request) {
        console.debug('Awaiting pending request', url);
        request.then((html) => {
            if (typeof html === 'string') {
                handleNavigation(url, html);
                return;
            }
            console.debug('Invalid response', url, html);
        });
        return false;
    }

    console.debug('no document for anchor');
}

// Setup link handlers using onAttr
setupLinkHandlers(initiateFetchDocument, navigate);
