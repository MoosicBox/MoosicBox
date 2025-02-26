import { handleError, onAttr } from './core';

onAttr('v-onclick', ({ element, attr }) => {
    attr = decodeURIComponent(attr);
    element.onclick = (event) => {
        event.stopPropagation();
        handleError('onclick', () => eval(attr));
        return false;
    };
});
