import { onMessage } from './core';

type CanvasAction =
    | 'clear'
    | { strokeColor: { r: number; g: number; b: number; a: number | null } }
    | { fillRect: [[number, number], [number, number]] }
    | { clearRect: [[number, number], [number, number]] };

type CanvasUpdate = {
    target: string;
    canvasActions: CanvasAction[];
};

onMessage('canvas_update', (data) => {
    const update = JSON.parse(data) as CanvasUpdate;

    const element = document.querySelector(`canvas#${update.target}`);
    if (!element) return;

    const canvas = element as HTMLCanvasElement;

    if (!canvas.dataset.vNoWidth) {
        canvas.dataset.vNoWidth = canvas.getAttribute('width')
            ? 'false'
            : 'true';
        canvas.dataset.vNoHeight = canvas.getAttribute('height')
            ? 'false'
            : 'true';
    }

    if (canvas.dataset.vNoWidth && canvas.width !== canvas.clientWidth) {
        canvas.width = canvas.clientWidth;
    }
    if (canvas.dataset.vNoHeight && canvas.height !== canvas.clientHeight) {
        canvas.height = canvas.clientHeight;
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
            ctx.clearRect(x1, y1, x2 - x1, y2 - y1);
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
});
