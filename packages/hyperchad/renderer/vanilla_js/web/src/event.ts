import { onMessage } from './core';

onMessage('event', (data) => {
    const splitIndex = data.indexOf(':');
    if (splitIndex === -1) return;
    const eventName = data.slice(0, splitIndex);
    const eventValue = data.slice(splitIndex + 1);
    const event = new CustomEvent(`v-${eventName}`, {
        detail: eventValue,
    });
    dispatchEvent(event);
});
