import { evaluate } from './actions';
import { handleError, onAttr } from './core';

onAttr('v-onload', ({ element, attr }) => {
    handleError('onload', () => evaluate(attr, { element }));
});
