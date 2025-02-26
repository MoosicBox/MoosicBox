import { evaluate } from './actions';
import { handleError, onAttr } from './core';

onAttr('v-onchange', ({ element, attr }) => {
    element.onchange = (event) => {
        handleError('onchange', () => evaluate(attr, { element, event }));
    };
});
