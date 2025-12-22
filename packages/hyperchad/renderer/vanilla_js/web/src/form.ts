import { appendQueryParams } from './core';
import { handleNavigation } from './nav-base';
import { processRoute } from './routing';

function initFormHandler() {
    document.body.addEventListener(
        'submit',
        async (e) => {
            const element = e.target as HTMLElement;

            if (!(element instanceof HTMLFormElement)) return;

            const hasHxRoute = ['get', 'post', 'put', 'delete', 'patch'].some(
                (method) => element.hasAttribute(`hx-${method}`),
            );
            const action = element.getAttribute('action');

            // If no hx-* and no action, let browser handle natively
            if (!hasHxRoute && !action) return;

            e.preventDefault();

            // Build form data
            let form: HTMLFormElement | null = element;
            const formData = new FormData();

            while (form) {
                const current = new FormData(form);

                for (const pair of current.entries()) {
                    formData.append(pair[0], pair[1]);
                }

                // TODO: Add support for no inheritance form
                form = form.parentElement?.closest('form') ?? null;
            }

            // Step 1: Execute hx-* route first (if present)
            if (hasHxRoute) {
                await processRoute(element, { body: formData });
            }

            // Step 2: Execute action navigation (if present)
            if (action) {
                const method = (
                    element.getAttribute('method') || 'GET'
                ).toUpperCase();
                const fetchOptions: RequestInit = {
                    method,
                };

                let url = action;
                if (method === 'GET') {
                    url = appendQueryParams(action, formData);
                } else {
                    // POST/PUT/etc - send form data in body
                    fetchOptions.body = formData;
                }

                // Fetch and trigger swapDom (same as anchor navigation)
                const response = await fetch(url, fetchOptions);
                const html = await response.text();

                handleNavigation(url, html);
            }

            return false;
        },
        true,
    );
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initFormHandler);
} else {
    initFormHandler();
}
