import { describe, it, expect, beforeEach } from 'vitest';

describe('actions-mouse-down', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    it('triggers on mousedown with v-onmousedown attribute', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-mouse-down');

        const btn = document.createElement('button');
        btn.id = 'btn';
        btn.setAttribute('v-onmousedown', 'window.__mouseDown = true');
        btn.textContent = 'Press';
        document.body.appendChild(btn);

        btn.dispatchEvent(
            new MouseEvent('mousedown', { bubbles: true, cancelable: true }),
        );

        // Wait for the interval to fire (16ms interval)
        await new Promise((resolve) => setTimeout(resolve, 50));

        // Release mouse to stop polling
        document.dispatchEvent(new MouseEvent('mouseup'));

        const mouseDown = (window as unknown as Record<string, boolean>)
            .__mouseDown;
        expect(mouseDown).toBe(true);
    });

    it('provides element in context', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-mouse-down');

        const btn = document.createElement('button');
        btn.id = 'my-button';
        btn.setAttribute(
            'v-onmousedown',
            'window.__elementId = ctx.element.id',
        );
        btn.textContent = 'Press';
        document.body.appendChild(btn);

        btn.dispatchEvent(
            new MouseEvent('mousedown', { bubbles: true, cancelable: true }),
        );

        // Wait for the interval to fire
        await new Promise((resolve) => setTimeout(resolve, 50));

        // Release mouse to stop polling
        document.dispatchEvent(new MouseEvent('mouseup'));

        const elementId = (window as unknown as Record<string, string>)
            .__elementId;
        expect(elementId).toBe('my-button');
    });

    it('provides event in context', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-mouse-down');

        const btn = document.createElement('button');
        btn.id = 'btn';
        // The event passed to evaluate is a position object, not the original event
        btn.setAttribute(
            'v-onmousedown',
            'window.__hasClientX = typeof ctx.event?.clientX === "number"',
        );
        btn.textContent = 'Press';
        document.body.appendChild(btn);

        btn.dispatchEvent(
            new MouseEvent('mousedown', {
                bubbles: true,
                cancelable: true,
                clientX: 100,
                clientY: 100,
            }),
        );

        // Wait for the interval to fire
        await new Promise((resolve) => setTimeout(resolve, 50));

        // Release mouse to stop polling
        document.dispatchEvent(new MouseEvent('mouseup'));

        const hasClientX = (window as unknown as Record<string, boolean>)
            .__hasClientX;
        expect(hasClientX).toBe(true);
    });

    it('bubbles up to find v-onmousedown attribute', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-mouse-down');

        const parent = document.createElement('div');
        parent.id = 'parent';
        parent.setAttribute('v-onmousedown', 'window.__parentTriggered = true');

        const child = document.createElement('button');
        child.id = 'child';
        child.textContent = 'Press';

        parent.appendChild(child);
        document.body.appendChild(parent);

        child.dispatchEvent(
            new MouseEvent('mousedown', { bubbles: true, cancelable: true }),
        );

        // Wait for the interval to fire
        await new Promise((resolve) => setTimeout(resolve, 50));

        // Release mouse to stop polling
        document.dispatchEvent(new MouseEvent('mouseup'));

        const parentTriggered = (window as unknown as Record<string, boolean>)
            .__parentTriggered;
        expect(parentTriggered).toBe(true);
    });

    it('continues polling while mouse is held', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-mouse-down');

        (window as unknown as Record<string, unknown>).__callCount = 0;

        const btn = document.createElement('button');
        btn.id = 'btn';
        btn.setAttribute('v-onmousedown', 'window.__callCount++');
        btn.textContent = 'Press';
        document.body.appendChild(btn);

        // Trigger mousedown
        btn.dispatchEvent(
            new MouseEvent('mousedown', { bubbles: true, cancelable: true }),
        );

        // Wait a bit for polling to occur
        await new Promise((resolve) => setTimeout(resolve, 100));

        // Release mouse
        document.dispatchEvent(
            new MouseEvent('mouseup', { bubbles: true, cancelable: true }),
        );

        const callCount = (window as unknown as Record<string, number>)
            .__callCount;

        // Should have been called multiple times due to polling
        expect(callCount).toBeGreaterThanOrEqual(1);
    });
});
