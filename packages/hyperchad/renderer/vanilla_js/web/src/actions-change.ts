import { evaluate } from './actions';
import { handleError, onAttr } from './core';

onAttr('v-onchange', ({ element, attr }) => {
    const eventName =
        element.getAttribute('type') === 'text' ? 'oninput' : 'onchange';
    element[eventName] = (event) => {
        const value = (event.target as HTMLInputElement).value;
        handleError(eventName, () => evaluate(attr, { element, event, value }));
    };
});
