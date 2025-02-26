import { handleError, onAttr } from './core';

// eslint-disable-next-line @typescript-eslint/no-unused-vars
onAttr('v-onload', ({ element, attr }) => {
    handleError('onload', () => eval(attr));
});
