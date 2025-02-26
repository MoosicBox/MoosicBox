import { evaluate } from './actions';
import { handleError, onAttr } from './core';

onAttr('v-onmouseover', ({ element, attr }) => {
    element.onmouseenter = (event) => {
        const reset = handleError('onmouseenter', () =>
            evaluate<string>(attr, { element, event }),
        );
        if (reset) {
            element.onmouseleave = (event) => {
                handleError('onmouseleave', () =>
                    evaluate(reset, { element, event }),
                );
            };
        }
    };
});
