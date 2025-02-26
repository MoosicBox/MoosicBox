import { handleError, onAttr } from './core';

onAttr('v-onmouseover', ({ element, attr }) => {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    element.onmouseenter = (event) => {
        const reset = handleError('onmouseenter', () => eval(attr));
        if (reset) {
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            element.onmouseleave = (event) => {
                handleError('onmouseleave', () => eval(reset));
            };
        }
    };
});
