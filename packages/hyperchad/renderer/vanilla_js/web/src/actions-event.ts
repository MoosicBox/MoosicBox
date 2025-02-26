import { evaluate } from './actions';
import { handleError, onAttr } from './core';

onAttr('v-onevent', ({ element, attr }) => {
    const eventNameSplitIndex = attr.indexOf(':');
    const eventName = attr.slice(0, eventNameSplitIndex);
    const eventAction = attr.slice(eventNameSplitIndex + 1);
    window.addEventListener(`v-${eventName}`, (event) => {
        const c = { element, event } as Parameters<typeof evaluate>[1];
        if ('detail' in event) c.value = event.detail;
        handleError('onevent', () => evaluate(eventAction, c));
    });
});
