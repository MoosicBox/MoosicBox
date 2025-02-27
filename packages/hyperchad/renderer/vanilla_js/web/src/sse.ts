import { htmlToElement, swapDom, triggerHandlers } from './core';
import { fetchEventSource } from './fetch-event-source';

type CanvasAction =
    | 'clear'
    | { strokeColor: { r: number; g: number; b: number; a: number | null } }
    | { fillRect: [[number, number], [number, number]] }
    | { clearRect: [[number, number], [number, number]] };

type CanvasUpdate = {
    target: string;
    canvasActions: CanvasAction[];
};

fetchEventSource('$sse', {
    method: 'GET',
    onopen: async (response: Response) => {
        if (response.status >= 400) {
            const status = response.status.toString();
            console.error('Failed to open SSE', { status });
        }
    },
    onmessage: (e) => {
        switch (e.event) {
            case 'view': {
                swapDom(e.data);
                break;
            }
            case 'partial_view': {
                const element = htmlToElement(e.data);
                if (element.children.length === 1) {
                    const replacement = element.children[0] as HTMLElement;
                    const target = document.getElementById(
                        element.children[0].id,
                    );
                    if (target) {
                        target.replaceWith(replacement);
                        triggerHandlers('domLoad', {
                            element: replacement,
                            initial: false,
                            navigation: false,
                        });
                    }
                }
                break;
            }
            case 'canvas_update': {
                const update = JSON.parse(e.data) as CanvasUpdate;

                const element = document.querySelector(
                    `canvas#${update.target}`,
                );
                if (!element) return;

                const canvas = element as HTMLCanvasElement;

                const attrWidth = canvas.getAttribute('width');
                const attrHeight = canvas.getAttribute('height');

                if (!attrWidth || canvas.dataset.vNoWidth === 'true') {
                    canvas.width = canvas.clientWidth;
                    canvas.dataset.vNoWidth = 'true';
                }
                if (!attrHeight || canvas.dataset.vNoHeight === 'true') {
                    canvas.height = canvas.clientHeight;
                    canvas.dataset.vNoHeight = 'true';
                }

                const ctx = canvas.getContext('2d');
                if (!ctx) return;

                for (const action of update.canvasActions) {
                    if (action === 'clear') {
                        ctx.clearRect(0, 0, canvas.width, canvas.height);
                        continue;
                    }
                    if ('clearRect' in action) {
                        const [[x1, y1], [x2, y2]] = action.clearRect;
                        ctx.clearRect(x1, y1, x2, y2);
                        continue;
                    }
                    if ('strokeColor' in action) {
                        const r = action.strokeColor.r;
                        const g = action.strokeColor.g;
                        const b = action.strokeColor.b;
                        const a =
                            typeof action.strokeColor.a === 'number'
                                ? action.strokeColor.a
                                : 1;

                        ctx.fillStyle = `rgba(${r},${g},${b},${a})`;
                        continue;
                    }
                    if ('fillRect' in action) {
                        const [[x1, y1], [x2, y2]] = action.fillRect;
                        ctx.fillRect(x1, y1, x2 - x1, y2 - y1);
                        continue;
                    }
                }

                break;
            }
            case 'event': {
                const splitIndex = e.data.indexOf(':');
                if (splitIndex === -1) return;
                const eventName = e.data.slice(0, splitIndex);
                const eventValue = e.data.slice(splitIndex + 1);
                const event = new CustomEvent(`v-${eventName}`, {
                    detail: eventValue,
                });
                dispatchEvent(event);
                break;
            }
        }
    },
    onerror: (error) => {
        if (error) {
            if (typeof error === 'object' && 'message' in error) {
                console.error('SSE error', error.message);
            } else {
                console.error('SSE error', error);
            }
        } else {
            console.error('SSE error', error);
        }
    },
});
