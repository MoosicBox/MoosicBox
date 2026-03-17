import { describe, it, expect, beforeEach } from 'vitest';

describe('actions-event', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    it('triggers on custom event with v-onevent attribute', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-event');

        clearProcessedElements();

        const div = document.createElement('div');
        div.id = 'target';
        div.setAttribute(
            'v-onevent',
            'my-custom-event:window.__eventTriggered = true',
        );
        document.body.appendChild(div);

        processElement(div, true);

        // Dispatch custom event on window with v- prefix (as the source expects)
        window.dispatchEvent(
            new CustomEvent('v-my-custom-event', { bubbles: true }),
        );

        const eventTriggered = (window as unknown as Record<string, boolean>)
            .__eventTriggered;
        expect(eventTriggered).toBe(true);
    });

    it('parses event name from attribute value', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-event');

        clearProcessedElements();

        const div = document.createElement('div');
        div.id = 'target';
        div.setAttribute(
            'v-onevent',
            'specific-event:window.__specificEventTriggered = true',
        );
        document.body.appendChild(div);

        processElement(div, true);

        // Dispatch a different event (should not trigger)
        window.dispatchEvent(
            new CustomEvent('v-other-event', { bubbles: true }),
        );

        let triggered = (window as unknown as Record<string, boolean>)
            .__specificEventTriggered;
        expect(triggered).toBeUndefined();

        // Dispatch the correct event
        window.dispatchEvent(
            new CustomEvent('v-specific-event', { bubbles: true }),
        );

        triggered = (window as unknown as Record<string, boolean>)
            .__specificEventTriggered;
        expect(triggered).toBe(true);
    });

    it('provides element in context', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-event');

        clearProcessedElements();

        const div = document.createElement('div');
        div.id = 'my-element';
        div.setAttribute(
            'v-onevent',
            'test-event:window.__elementId = ctx.element.id',
        );
        document.body.appendChild(div);

        processElement(div, true);

        window.dispatchEvent(
            new CustomEvent('v-test-event', { bubbles: true }),
        );

        const elementId = (window as unknown as Record<string, string>)
            .__elementId;
        expect(elementId).toBe('my-element');
    });

    it('provides event detail in context value', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-event');

        clearProcessedElements();

        const div = document.createElement('div');
        div.id = 'target';
        div.setAttribute(
            'v-onevent',
            'data-event:window.__eventDetail = ctx.value',
        );
        document.body.appendChild(div);

        processElement(div, true);

        window.dispatchEvent(
            new CustomEvent('v-data-event', {
                bubbles: true,
                detail: 'hello',
            }),
        );

        const detail = (window as unknown as Record<string, string>)
            .__eventDetail;
        expect(detail).toBe('hello');
    });
});
