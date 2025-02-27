import { evaluate } from './actions';
import { handleError, onAttr } from './core';

let mousePos: { x: number; y: number } | undefined;
const intervals: Set<ReturnType<typeof setInterval>> = new Set();

window.addEventListener('mousemove', (event: MouseEvent) => {
    if (!mousePos) mousePos = {} as unknown as typeof mousePos;
    mousePos!.x = event.clientX;
    mousePos!.y = event.clientY;
});
window.addEventListener('mouseup', () => {
    intervals.forEach(clearInterval);
    intervals.clear();
});

onAttr('v-onmousedown', ({ element, attr }) => {
    element.onmousedown = (event) => {
        const pos: { clientX: number; clientY: number } = {
            clientX: event.clientX,
            clientY: event.clientY,
        };
        intervals.add(
            setInterval(() => {
                if (mousePos) {
                    pos.clientX = mousePos.x;
                    pos.clientY = mousePos.y;
                }
                handleError('onmousedown', () =>
                    evaluate(attr, { element, event: pos as unknown as Event }),
                );
            }, 16),
        );
    };
});
