import { onAttr } from './core';

onAttr('v-onload', ({ element, attr }) => {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    element.onload = (event) => {
        try {
            eval(attr);
        } catch (e) {
            console.error('onload failed', e);
        }
    };
});
