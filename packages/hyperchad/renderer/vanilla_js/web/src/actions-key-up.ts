import { evaluate } from './actions';
import { handleError, decodeHtml, createEventDelegator } from './core';

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
