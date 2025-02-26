import { onAttr } from './core';

onAttr('v-onclick', ({ element, attr }) => {
    element.onclick = (event) => {
        event.stopPropagation();
        try {
            eval(attr);
        } catch (e) {
            console.error('onclick failed', e);
        }
        return false;
    };
});
