import { onAttr } from './core';

onAttr('v-onmousedown', ({ element, attr }) => {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    element.onmousedown = (event) => {
        try {
            eval(attr);
        } catch (e) {
            console.error('onmousedown failed', e);
        }
    };
});
