import { handleError, onAttr } from './core';

onAttr('v-onmousedown', ({ element, attr }) => {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    element.onmousedown = (event) => {
        handleError('onmousedown', () => eval(attr));
    };
});
