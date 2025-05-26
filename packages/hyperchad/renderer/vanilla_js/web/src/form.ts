import { onAttrValue } from './core';
import { processRoute } from './routing';

onAttrValue('type', 'submit', ({ element }) => {
    element.addEventListener('submit', (e) => {
        e.preventDefault();

        let form = element.closest('form');
        const formData = new FormData();

        while (form) {
            const current = new FormData(form);

            for (const pair of current.entries()) {
                formData.append(pair[0], pair[1]);
            }

            // TODO: Add support for no inheritance form
            form = form.parentElement?.closest('form') ?? null;
        }

        processRoute(element, { body: formData });
    });
});
