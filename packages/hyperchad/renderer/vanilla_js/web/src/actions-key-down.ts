import { evaluate } from './actions';
import { handleError, decodeHtml, createEventDelegator } from './core';

createEventDelegator('keydown', 'v-onkeydown', (element, attr, event) => {
    const keyEvent = event as KeyboardEvent;
    handleError('onkeydown', () =>
        evaluate(decodeHtml(attr), {
            element,
            event: keyEvent,
            value: keyEvent.key,
        }),
    );
});
