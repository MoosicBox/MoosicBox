import { onAttr } from './core';

onAttr('v-onmouseover', ({ element, attr }) => {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    element.onmouseenter = (event) => {
        try {
            const reset = eval(attr);
            if (reset) {
                // eslint-disable-next-line @typescript-eslint/no-unused-vars
                element.onmouseleave = (event) => {
                    try {
                        eval(reset);
                    } catch (e) {
                        console.error('onmouseleave failed', e);
                    }
                };
            }
        } catch (e) {
            console.error('onmouseenter failed', e);
        }
    };
});
