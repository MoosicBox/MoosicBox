import { evaluate, createEventDelegator } from './actions';
import { handleError, decodeHtml } from './core';

createEventDelegator('keyup', 'v-onkeyup', (element, attr, event) => {
    const keyEvent = event as KeyboardEvent;
    handleError('onkeyup', () =>
        evaluate(decodeHtml(attr), {
            element,
            event: keyEvent,
            value: keyEvent.key,
        }),
    );
});
