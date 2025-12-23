import { describe, it, expect, beforeEach } from 'vitest';

describe('actions-key-up', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    it('triggers on keyup with v-onkeyup attribute', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-key-up');

        const input = document.createElement('input');
        input.id = 'input';
        input.setAttribute('v-onkeyup', 'window.__keyReleased = true');
        document.body.appendChild(input);

        input.dispatchEvent(
            new KeyboardEvent('keyup', { key: 'a', bubbles: true }),
        );

        const keyReleased = (window as unknown as Record<string, boolean>)
            .__keyReleased;
        expect(keyReleased).toBe(true);
    });

    it('provides key value in context', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-key-up');

        const input = document.createElement('input');
        input.id = 'input';
        input.setAttribute('v-onkeyup', 'window.__key = ctx.value');
        document.body.appendChild(input);

        input.dispatchEvent(
            new KeyboardEvent('keyup', { key: 'Escape', bubbles: true }),
        );

        const key = (window as unknown as Record<string, string>).__key;
        expect(key).toBe('Escape');
    });

    it('provides element in context', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-key-up');

        const input = document.createElement('input');
        input.id = 'my-input';
        input.setAttribute('v-onkeyup', 'window.__elementId = ctx.element.id');
        document.body.appendChild(input);

        input.dispatchEvent(
            new KeyboardEvent('keyup', { key: 'a', bubbles: true }),
        );

        const elementId = (window as unknown as Record<string, string>)
            .__elementId;
        expect(elementId).toBe('my-input');
    });
});
