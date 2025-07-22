import { evaluate } from './actions';
import { handleError, onAttr } from './core';

const mouseOverElements: {
    element: HTMLElement;
    attr: string;
    reset?: string;
}[] = [];

document.addEventListener(
    'mouseenter',
    (event: MouseEvent) => {
        for (let i = mouseOverElements.length - 1; i >= 0; i--) {
            const entry = mouseOverElements[i];
            const { element, attr } = entry;
            if (!document.contains(element)) {
                mouseOverElements.splice(i, 1);
                continue;
            }

            if (
                element === event.target ||
                element.contains(event.target as Node)
            ) {
                const reset = handleError('onmouseenter', () =>
                    evaluate<string>(attr, { element, event }),
                );
                if (reset) {
                    entry.reset = reset;
                }
            }
        }
    },
    true,
);

document.addEventListener(
    'mouseleave',
    (event: MouseEvent) => {
        for (let i = mouseOverElements.length - 1; i >= 0; i--) {
            const entry = mouseOverElements[i];
            const { element, reset } = entry;
            if (!document.contains(element)) {
                mouseOverElements.splice(i, 1);
                continue;
            }

            if (
                reset &&
                (element === event.target ||
                    element.contains(event.target as Node))
            ) {
                handleError('onmouseleave', () =>
                    evaluate(reset, { element, event }),
                );
                entry.reset = undefined;
            }
        }
    },
    true,
);

onAttr('v-onmouseover', ({ element, attr }) => {
    mouseOverElements.push({ element, attr });
});
