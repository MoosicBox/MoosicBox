import { evaluate, createEventDelegator } from './actions';
import { handleError, decodeHtml } from './core';

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
