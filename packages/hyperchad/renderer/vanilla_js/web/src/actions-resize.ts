import { evaluate } from './actions';
import { handleError, onAttr } from './core';

const resizeElements: { element: HTMLElement; f: (event: Event) => void }[] =
    [];

window.addEventListener('resize', (event) => {
    for (let i = resizeElements.length - 1; i >= 0; i--) {
        const { element, f } = resizeElements[i];
        if (!document.contains(element)) {
            resizeElements.splice(i, 1);
            continue;
        }

        f(event);
    }
});

onAttr('v-onresize', ({ element, attr }) => {
    let lastWidth = element.clientWidth;
    let lastHeight = element.clientHeight;

    resizeElements.push({
        element,
        f: (event) => {
            // FIXME: unsubscribe from this when element detached
            let resized = false;

            if (element.clientWidth !== lastWidth) {
                resized = true;
                lastWidth = element.clientWidth;
            }
            if (element.clientHeight !== lastHeight) {
                resized = true;
                lastHeight = element.clientHeight;
            }

            if (!resized) return;

            handleError('onresize', () => evaluate(attr, { element, event }));
        },
    });
});
