import { evaluate } from './actions';
import { handleError, onAttr } from './core';

const clickOutsideElements: {
    element: HTMLElement;
    f: (event: Event) => void;
}[] = [];

document.addEventListener('click', (event: MouseEvent) => {
    for (let i = clickOutsideElements.length - 1; i >= 0; i--) {
        const { element, f } = clickOutsideElements[i];
        if (!document.contains(element)) {
            clickOutsideElements.splice(i, 1);
            continue;
        }

        f(event);
    }
});

onAttr('v-onclickoutside', ({ element, attr }) => {
    clickOutsideElements.push({
        element,
        f: (event) => {
            if (event.target && !element.contains(event.target as Node)) {
                handleError('onclickoutside', () =>
                    evaluate(attr, { element, event }),
                );
            }
        },
    });
});
