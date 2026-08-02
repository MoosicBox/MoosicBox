import { evaluate } from './actions';
import { handleError, onAttr } from './core';

const actionsByElement = new WeakMap<HTMLElement, Map<string, string>>();

onAttr('v-onevent', ({ element, attr }) => {
    const eventNameSplitIndex = attr.indexOf(':');
    const eventName = attr.slice(0, eventNameSplitIndex);
    const eventAction = attr.slice(eventNameSplitIndex + 1);
    let actions = actionsByElement.get(element);
    if (!actions) {
        actions = new Map<string, string>();
        actionsByElement.set(element, actions);
    }

    if (!actions.has(eventName)) {
        window.addEventListener(`v-${eventName}`, (event) => {
            if (!element.isConnected) return;
            const currentAction = actions?.get(eventName);
            if (!currentAction) return;
            const c = { element, event } as Parameters<typeof evaluate>[1];
            if ('detail' in event) c.value = event.detail;
            handleError('onevent', () => evaluate(currentAction, c));
        });
    }

    actions.set(eventName, eventAction);
});
