import { describe, it, expect, beforeEach } from 'vitest';

describe('actions-key-down', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    it('triggers on keydown with v-onkeydown attribute', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-key-down');

        const input = document.createElement('input');
        input.id = 'input';
        input.setAttribute('v-onkeydown', 'window.__keyPressed = true');
        document.body.appendChild(input);

        input.dispatchEvent(
            new KeyboardEvent('keydown', { key: 'a', bubbles: true }),
        );

        const keyPressed = (window as unknown as Record<string, boolean>)
            .__keyPressed;
        expect(keyPressed).toBe(true);
    });

    it('provides key value in context', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-key-down');

        const input = document.createElement('input');
        input.id = 'input';
        input.setAttribute('v-onkeydown', 'window.__key = ctx.value');
        document.body.appendChild(input);

        input.dispatchEvent(
            new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }),
        );

        const key = (window as unknown as Record<string, string>).__key;
        expect(key).toBe('Enter');
    });

    it('provides element in context', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-key-down');

        const input = document.createElement('input');
        input.id = 'my-input';
        input.setAttribute(
            'v-onkeydown',
            'window.__elementId = ctx.element.id',
        );
        document.body.appendChild(input);

        input.dispatchEvent(
            new KeyboardEvent('keydown', { key: 'a', bubbles: true }),
        );

        const elementId = (window as unknown as Record<string, string>)
            .__elementId;
        expect(elementId).toBe('my-input');
    });

    it('provides event in context', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-key-down');

        const input = document.createElement('input');
        input.id = 'input';
        input.setAttribute(
            'v-onkeydown',
            'window.__eventType = ctx.event?.type',
        );
        document.body.appendChild(input);

        input.dispatchEvent(
            new KeyboardEvent('keydown', { key: 'a', bubbles: true }),
        );

        const eventType = (window as unknown as Record<string, string>)
            .__eventType;
        expect(eventType).toBe('keydown');
    });

    it('bubbles up to find v-onkeydown attribute', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-key-down');

        const parent = document.createElement('div');
        parent.id = 'parent';
        parent.setAttribute('v-onkeydown', 'window.__parentTriggered = true');

        const input = document.createElement('input');
        input.id = 'child';

        parent.appendChild(input);
        document.body.appendChild(parent);

        input.dispatchEvent(
            new KeyboardEvent('keydown', { key: 'a', bubbles: true }),
        );

        const parentTriggered = (window as unknown as Record<string, boolean>)
            .__parentTriggered;
        expect(parentTriggered).toBe(true);
    });
});
