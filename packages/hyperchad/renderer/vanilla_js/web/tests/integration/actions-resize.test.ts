import { describe, it, expect, beforeEach } from 'vitest';

describe('actions-resize', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    it('triggers on window resize with v-onresize attribute', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-resize');

        clearProcessedElements();

        const div = document.createElement('div');
        div.id = 'resize-target';
        div.style.width = '100px';
        div.style.height = '100px';
        div.setAttribute('v-onresize', 'window.__resized = true');
        document.body.appendChild(div);

        processElement(div, true);

        // Simulate dimension change by modifying the element's style
        div.style.width = '200px';

        // Trigger resize event
        window.dispatchEvent(new Event('resize'));

        const resized = (window as unknown as Record<string, boolean>)
            .__resized;
        expect(resized).toBe(true);
    });

    it('provides element in context', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-resize');

        clearProcessedElements();

        const div = document.createElement('div');
        div.id = 'my-element';
        div.style.width = '100px';
        div.style.height = '100px';
        div.setAttribute('v-onresize', 'window.__elementId = ctx.element.id');
        document.body.appendChild(div);

        processElement(div, true);

        // Simulate dimension change
        div.style.width = '200px';

        window.dispatchEvent(new Event('resize'));

        const elementId = (window as unknown as Record<string, string>)
            .__elementId;
        expect(elementId).toBe('my-element');
    });

    it('provides event in context', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-resize');

        clearProcessedElements();

        const div = document.createElement('div');
        div.id = 'sized-element';
        div.style.width = '100px';
        div.style.height = '100px';
        div.setAttribute('v-onresize', 'window.__eventType = ctx.event?.type');
        document.body.appendChild(div);

        processElement(div, true);

        // Simulate dimension change
        div.style.width = '200px';

        window.dispatchEvent(new Event('resize'));

        const eventType = (window as unknown as Record<string, string>)
            .__eventType;
        expect(eventType).toBe('resize');
    });

    it('triggers for multiple elements with v-onresize', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-resize');

        clearProcessedElements();

        (window as unknown as Record<string, unknown>).__resizeCount = 0;

        const div1 = document.createElement('div');
        div1.id = 'resize-1';
        div1.style.width = '100px';
        div1.style.height = '100px';
        div1.setAttribute('v-onresize', 'window.__resizeCount++');
        document.body.appendChild(div1);

        const div2 = document.createElement('div');
        div2.id = 'resize-2';
        div2.style.width = '100px';
        div2.style.height = '100px';
        div2.setAttribute('v-onresize', 'window.__resizeCount++');
        document.body.appendChild(div2);

        processElement(div1, true);
        processElement(div2, true);

        // Simulate dimension change for both
        div1.style.width = '200px';
        div2.style.width = '200px';

        window.dispatchEvent(new Event('resize'));

        const resizeCount = (window as unknown as Record<string, number>)
            .__resizeCount;

        // Both elements should have had their handlers called
        expect(resizeCount).toBe(2);
    });
});
