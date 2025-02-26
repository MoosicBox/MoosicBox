import { handleError, onAttr } from './core';

onAttr('v-onresize', ({ element, attr }) => {
    let lastWidth = element.clientWidth;
    let lastHeight = element.clientHeight;

    // FIXME: unsubscribe from this when element detached
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    window.addEventListener('resize', (event) => {
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

        handleError('onresize', () => eval(attr));
    });
});
