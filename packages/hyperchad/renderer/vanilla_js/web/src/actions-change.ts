import { handleError, onAttr } from './core';

onAttr('v-onchange', ({ element, attr }) => {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    element.onchange = (event) => {
        handleError('onchange', () => eval(attr));
    };
});
