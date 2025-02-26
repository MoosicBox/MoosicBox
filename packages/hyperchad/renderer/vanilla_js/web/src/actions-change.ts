import { onAttr } from './core';

onAttr('v-onchange', ({ element, attr }) => {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    element.onchange = (event) => {
        try {
            eval(attr);
        } catch (e) {
            console.error('onchange failed', e);
        }
    };
});
