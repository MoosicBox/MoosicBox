import { describe, it, expect, beforeEach } from 'vitest';

describe('actions-immediate', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    it('executes v-onload immediately when element is processed', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-immediate');

        clearProcessedElements();

        (window as unknown as Record<string, unknown>).__loadTriggered = false;

        const div = document.createElement('div');
        div.id = 'test';
        div.setAttribute('v-onload', 'window.__loadTriggered = true');
        document.body.appendChild(div);

        processElement(div, true);

        const triggered = (window as unknown as Record<string, boolean>)
            .__loadTriggered;
        expect(triggered).toBe(true);
    });

    it('provides element in context', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-immediate');

        clearProcessedElements();

        const div = document.createElement('div');
        div.id = 'my-element';
        div.setAttribute('v-onload', 'window.__elementId = ctx.element.id');
        document.body.appendChild(div);

        processElement(div, true);

        const elementId = (window as unknown as Record<string, string>)
            .__elementId;
        expect(elementId).toBe('my-element');
    });

    it('executes for nested elements', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-immediate');

        clearProcessedElements();

        (window as unknown as Record<string, unknown>).__loadOrder = [];

        const parent = document.createElement('div');
        parent.id = 'parent';
        parent.setAttribute('v-onload', "window.__loadOrder.push('parent')");

        const child = document.createElement('div');
        child.id = 'child';
        child.setAttribute('v-onload', "window.__loadOrder.push('child')");

        parent.appendChild(child);
        document.body.appendChild(parent);

        processElement(parent, true);

        const loadOrder = (window as unknown as Record<string, string[]>)
            .__loadOrder;
        expect(loadOrder).toContain('parent');
        expect(loadOrder).toContain('child');
    });

    it('handles errors gracefully', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-immediate');

        clearProcessedElements();

        const errors: string[] = [];
        const originalError = console.error;
        console.error = (...args: unknown[]) => {
            errors.push(args.join(' '));
            originalError.apply(console, args);
        };

        const div = document.createElement('div');
        div.id = 'test';
        div.setAttribute('v-onload', 'throw new Error("test error")');
        document.body.appendChild(div);

        processElement(div, true);

        // Restore console.error
        console.error = originalError;

        expect(
            errors.some((e) => e.includes('onload') || e.includes('failed')),
        ).toBe(true);
    });
});
