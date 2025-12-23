import { processRoute } from './routing';

function initFormHandler() {
    document.body.addEventListener(
        'submit',
        (e) => {
            const element = e.target as HTMLElement;

            if (!(element instanceof HTMLFormElement)) return;

            let form = e.target as HTMLFormElement | null;

            e.preventDefault();

            const formData = new FormData();

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
        },
        true,
    );
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initFormHandler);
} else {
    initFormHandler();
}
