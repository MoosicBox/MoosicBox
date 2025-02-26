import { evaluate } from './actions';
import { handleError, onAttr } from './core';

onAttr('v-onmousedown', ({ element, attr }) => {
    element.onmousedown = (event) => {
        handleError('onmousedown', () => evaluate(attr, { element, event }));
    };
});
