import { evaluate } from './actions';
import { handleError, onAttr } from './core';

onAttr('v-onclickoutside', ({ element, attr }) => {
    // FIXME: unsubscribe from this when element detached
    document.addEventListener('click', (event: MouseEvent) => {
        if (event.target && !element.contains(event.target as Node)) {
            handleError('onclickoutside', () =>
                evaluate(attr, { element, event }),
            );
        }
    });
});
