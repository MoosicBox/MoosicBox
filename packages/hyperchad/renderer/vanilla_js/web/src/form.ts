import { onElement } from './core';
import { processRoute } from './routing';

onElement(({ element }) => {
    if (!(element instanceof HTMLFormElement)) return;

    element.addEventListener('submit', (e) => {
        e.preventDefault();

        const formData = new FormData();

        let form: HTMLFormElement | null = element;

        while (form) {
            const current = new FormData(form);

            for (const pair of current.entries()) {
                formData.append(pair[0], pair[1]);
            }

            // TODO: Add support for no inheritance form
            form = form.parentElement?.closest('form') ?? null;
        }

        processRoute(element, {
            body: formData,
        });

        return false;
    });
});
