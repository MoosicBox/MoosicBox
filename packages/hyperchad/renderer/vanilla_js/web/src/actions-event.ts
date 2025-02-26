import { onAttr } from './core';

onAttr('v-onevent', (event) => {
    const { attr } = event;
    const eventNameSplitIndex = attr.indexOf(':');
    const eventName = attr.slice(0, eventNameSplitIndex);
    const eventAction = attr.slice(eventNameSplitIndex + 1);
    window.addEventListener(`v-${eventName}`, (event) => {
        let value = null;
        if ('detail' in event) {
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            value = event.detail;
        }
        try {
            eval(eventAction);
        } catch (e) {
            console.error('onevent failed', e);
        }
    });
});
