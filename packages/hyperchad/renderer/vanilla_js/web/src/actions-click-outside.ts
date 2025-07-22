import { evaluate } from './actions';
import { handleError, decodeHtml } from './core';

document.addEventListener('click', (event: MouseEvent) => {
    // Find all elements with v-onclickoutside attribute
    const elementsWithClickOutside =
        document.querySelectorAll('[v-onclickoutside]');

    for (const element of elementsWithClickOutside) {
        if (event.target && !element.contains(event.target as Node)) {
            // Read the current attribute value from the element
            const attr = element.getAttribute('v-onclickoutside');
            if (attr) {
                handleError('onclickoutside', () =>
                    evaluate(decodeHtml(attr), {
                        element: element as HTMLElement,
                        event,
                    }),
                );
            }
        }
    }
});
