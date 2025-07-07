import { evaluate } from './actions';
import { handleError, onAttr } from './core';

onAttr('v-onkeyup', ({ element, attr }) => {
    element.onkeyup = (event) => {
        handleError('onkeyup', () =>
            evaluate(attr, {
                element,
                event: event,
                value: event.key,
            }),
        );
    };
});
