import { evaluate } from './actions';
import { handleError, decodeHtml, createEventDelegator } from './core';

createEventDelegator('input', 'v-onchange', (element, attr, event) => {
    const value = (event.target as HTMLInputElement).value;
    handleError('oninput', () =>
        evaluate(decodeHtml(attr), { element, event, value }),
    );
});

createEventDelegator('change', 'v-onchange', (element, attr, event) => {
    const value = (event.target as HTMLInputElement).value;
    handleError('onchange', () =>
        evaluate(decodeHtml(attr), { element, event, value }),
    );
});
