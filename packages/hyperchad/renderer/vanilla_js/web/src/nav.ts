import {
    cache,
    pending,
    handleNavigation,
    setupLinkHandlers,
} from './nav-base';

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
    const replace =
        url === `${window.location.pathname}${window.location.search}`;
    void initiateFetchDocument(url).then((html) => {
        if (typeof html === 'string') {
            handleNavigation(url, html, replace);
        }
    });
    return false;
}

declare global {
    interface Window {
        navigate: typeof navigate;
    }
}

window['navigate'] = navigate;

// Setup link handlers using onAttr
setupLinkHandlers(initiateFetchDocument, navigate);
