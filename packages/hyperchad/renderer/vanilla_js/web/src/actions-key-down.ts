import { evaluate } from './actions';
import { handleError, onAttr } from './core';

onAttr('v-onkeydown', ({ element, attr }) => {
    element.onkeydown = (event) => {
        handleError('onkeydown', () =>
            evaluate(attr, {
                element,
                event: event,
                value: event.key,
            }),
        );
    };
});
