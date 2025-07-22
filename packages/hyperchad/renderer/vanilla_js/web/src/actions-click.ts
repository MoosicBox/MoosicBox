import { evaluate, createEventDelegator } from './actions';
import { handleError } from './core';

createEventDelegator('click', 'v-onclick', (element, attr, event) => {
    event.stopPropagation();
    handleError('onclick', () =>
        evaluate(decodeURIComponent(attr), { element, event }),
    );
});
