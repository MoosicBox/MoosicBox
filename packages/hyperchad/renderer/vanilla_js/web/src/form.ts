import { appendQueryParams } from './core';
import { handleNavigation } from './nav-base';
import { processRoute, waitForPendingRoutes } from './routing';

const pendingSubmissions = new WeakSet<HTMLFormElement>();

function submitControls(
    form: HTMLFormElement,
): (HTMLButtonElement | HTMLInputElement)[] {
    return Array.from(
        form.querySelectorAll<HTMLButtonElement | HTMLInputElement>(
            'button:not([type]), button[type="submit"], input[type="submit"], input[type="image"]',
        ),
    );
}

function beginSubmission(form: HTMLFormElement): () => void {
    pendingSubmissions.add(form);
    form.setAttribute('aria-busy', 'true');
    const controls = submitControls(form).map((control) => ({
        control,
        disabled: control.disabled,
    }));
    controls.forEach(({ control }) => {
        control.disabled = true;
    });

    return () => {
        pendingSubmissions.delete(form);
        form.removeAttribute('aria-busy');
        controls.forEach(({ control, disabled }) => {
            control.disabled = disabled;
        });
    };
}

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

            if (pendingSubmissions.has(element)) return false;
            const endSubmission = beginSubmission(element);

            try {
                // Preserve request ordering when a change-triggered draft save is
                // still in flight for this form.
                await waitForPendingRoutes(element);

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
            } finally {
                endSubmission();
            }
        },
        true,
    );
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initFormHandler);
} else {
    initFormHandler();
}
