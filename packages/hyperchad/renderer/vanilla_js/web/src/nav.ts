import { onAttr, triggerHandlers } from './core';

const cache: { [url: string]: string } = {};
const pending: { [url: string]: Promise<string | void> } = {};

onAttr('href', ({ element, attr }) => {
    // Only handle links for this site
    if (attr[0] === '/') {
        element.addEventListener('mouseenter', (_event) => {
            const existing = typeof cache[attr] === 'string' || pending[attr];

            if (!existing) {
                pending[attr] = fetch(attr)
                    .then((response) => {
                        return response.text();
                    })
                    .then((html) => {
                        cache[attr] = html;
                        delete pending[attr];

                        return html;
                    })
                    .catch((e) => {
                        console.error('Failed to fetch document', attr, e);
                    });
            }
        });

        element.addEventListener('click', (event) => {
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
    }
});
