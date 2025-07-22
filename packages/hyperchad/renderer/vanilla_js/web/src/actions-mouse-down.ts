import { evaluate, createEventDelegator } from './actions';
import { handleError, decodeHtml } from './core';

let mousePos: { x: number; y: number } | undefined;
const intervals: Set<ReturnType<typeof setInterval>> = new Set();

document.addEventListener('mousemove', (event: MouseEvent) => {
    if (!mousePos) mousePos = {} as unknown as typeof mousePos;
    mousePos!.x = event.clientX;
    mousePos!.y = event.clientY;
});

document.addEventListener('mouseup', () => {
    intervals.forEach(clearInterval);
    intervals.clear();
});

createEventDelegator('mousedown', 'v-onmousedown', (element, attr, event) => {
    const mouseEvent = event as MouseEvent;
    const pos: { clientX: number; clientY: number } = {
        clientX: mouseEvent.clientX,
        clientY: mouseEvent.clientY,
    };
    intervals.add(
        setInterval(() => {
            if (mousePos) {
                pos.clientX = mousePos.x;
                pos.clientY = mousePos.y;
            }
            handleError('onmousedown', () =>
                evaluate(decodeHtml(attr), {
                    element,
                    event: pos as unknown as Event,
                }),
            );
        }, 16),
    );
});
