import { handleError, onAttr } from './core';

onAttr('v-onload', ({ element, attr }) => {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    element.onload = (event) => {
        handleError('onload', () => eval(attr));
    };
});
