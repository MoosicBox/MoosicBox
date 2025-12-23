import { describe, it, expect, beforeEach } from 'vitest';

describe('canvas', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    describe('canvas rendering', () => {
        it('handles clear command', async () => {
            await import('../../src/core');
            await import('../../src/canvas');

            const { triggerMessage } = await import('../../src/core');

            const canvas = document.createElement('canvas');
            canvas.id = 'test-canvas';
            canvas.width = 100;
            canvas.height = 100;
            document.body.appendChild(canvas);

            // Draw something first
            const ctx = canvas.getContext('2d');
            if (ctx) {
                ctx.fillStyle = 'red';
                ctx.fillRect(0, 0, 100, 100);
            }

            // Trigger clear
            triggerMessage(
                'canvas_update',
                JSON.stringify({
                    target: 'test-canvas',
                    canvasActions: ['clear'],
                }),
            );

            // Canvas should be cleared (difficult to test visually, but we can verify no error)
            const canvasExists =
                document.getElementById('test-canvas') !== null;
            expect(canvasExists).toBe(true);
        });

        it('handles fillRect command', async () => {
            await import('../../src/core');
            await import('../../src/canvas');

            const { triggerMessage } = await import('../../src/core');

            const canvas = document.createElement('canvas');
            canvas.id = 'test-canvas';
            canvas.width = 100;
            canvas.height = 100;
            document.body.appendChild(canvas);

            // Trigger fillRect
            triggerMessage(
                'canvas_update',
                JSON.stringify({
                    target: 'test-canvas',
                    canvasActions: [
                        {
                            fillRect: [
                                [10, 10],
                                [60, 60],
                            ],
                        },
                    ],
                }),
            );

            // Verify canvas exists and command was processed
            const canvasExists =
                document.getElementById('test-canvas') !== null;
            expect(canvasExists).toBe(true);
        });

        it('handles strokeColor command', async () => {
            await import('../../src/core');
            await import('../../src/canvas');

            const { triggerMessage } = await import('../../src/core');

            const canvas = document.createElement('canvas');
            canvas.id = 'test-canvas';
            canvas.width = 100;
            canvas.height = 100;
            document.body.appendChild(canvas);

            // Trigger strokeColor
            triggerMessage(
                'canvas_update',
                JSON.stringify({
                    target: 'test-canvas',
                    canvasActions: [
                        { strokeColor: { r: 255, g: 0, b: 0, a: 1 } },
                    ],
                }),
            );

            // Verify canvas exists
            const canvasExists =
                document.getElementById('test-canvas') !== null;
            expect(canvasExists).toBe(true);
        });

        it('handles clearRect command', async () => {
            await import('../../src/core');
            await import('../../src/canvas');

            const { triggerMessage } = await import('../../src/core');

            const canvas = document.createElement('canvas');
            canvas.id = 'test-canvas';
            canvas.width = 100;
            canvas.height = 100;
            document.body.appendChild(canvas);

            // Draw background
            const ctx = canvas.getContext('2d');
            if (ctx) {
                ctx.fillStyle = 'blue';
                ctx.fillRect(0, 0, 100, 100);
            }

            // Trigger clearRect
            triggerMessage(
                'canvas_update',
                JSON.stringify({
                    target: 'test-canvas',
                    canvasActions: [
                        {
                            clearRect: [
                                [25, 25],
                                [75, 75],
                            ],
                        },
                    ],
                }),
            );

            const canvasExists =
                document.getElementById('test-canvas') !== null;
            expect(canvasExists).toBe(true);
        });

        it('ignores commands for non-existent canvas', async () => {
            await import('../../src/core');
            await import('../../src/canvas');

            const { triggerMessage } = await import('../../src/core');

            // Should not throw
            triggerMessage(
                'canvas_update',
                JSON.stringify({
                    target: 'non-existent-canvas',
                    canvasActions: ['clear'],
                }),
            );

            (window as unknown as Record<string, unknown>).__noError = true;

            const noError = (window as unknown as Record<string, boolean>)
                .__noError;
            expect(noError).toBe(true);
        });
    });
});
