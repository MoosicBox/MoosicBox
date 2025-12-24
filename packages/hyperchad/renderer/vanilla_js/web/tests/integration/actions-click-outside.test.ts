import { describe, it, expect, beforeEach } from 'vitest';

describe('actions-click-outside', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    it('triggers when clicking outside element with v-onclickoutside', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-click-outside');

        clearProcessedElements();

        const target = document.createElement('div');
        target.id = 'target';
        target.setAttribute(
            'v-onclickoutside',
            'window.__clickedOutside = true',
        );
        target.style.width = '100px';
        target.style.height = '100px';
        document.body.appendChild(target);

        const outside = document.createElement('div');
        outside.id = 'outside';
        outside.style.width = '100px';
        outside.style.height = '100px';
        document.body.appendChild(outside);

        processElement(target, true);

        // Click outside the target
        outside.dispatchEvent(
            new MouseEvent('click', { bubbles: true, cancelable: true }),
        );

        const clickedOutside = (window as unknown as Record<string, boolean>)
            .__clickedOutside;
        expect(clickedOutside).toBe(true);
    });

    it('does NOT trigger when clicking inside element', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-click-outside');

        clearProcessedElements();

        (window as unknown as Record<string, unknown>).__clickedOutside = false;

        const target = document.createElement('div');
        target.id = 'target';
        target.setAttribute(
            'v-onclickoutside',
            'window.__clickedOutside = true',
        );
        target.style.width = '100px';
        target.style.height = '100px';
        document.body.appendChild(target);

        processElement(target, true);

        // Click inside the target
        target.dispatchEvent(
            new MouseEvent('click', { bubbles: true, cancelable: true }),
        );

        const clickedOutside = (window as unknown as Record<string, boolean>)
            .__clickedOutside;
        expect(clickedOutside).toBe(false);
    });

    it('does NOT trigger when clicking on child of element', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-click-outside');

        clearProcessedElements();

        (window as unknown as Record<string, unknown>).__clickedOutside = false;

        const target = document.createElement('div');
        target.id = 'target';
        target.setAttribute(
            'v-onclickoutside',
            'window.__clickedOutside = true',
        );

        const child = document.createElement('span');
        child.id = 'child';
        child.textContent = 'Child';

        target.appendChild(child);
        document.body.appendChild(target);

        processElement(target, true);

        // Click on child
        child.dispatchEvent(
            new MouseEvent('click', { bubbles: true, cancelable: true }),
        );

        const clickedOutside = (window as unknown as Record<string, boolean>)
            .__clickedOutside;
        expect(clickedOutside).toBe(false);
    });

    it('provides element in context', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-click-outside');

        clearProcessedElements();

        const target = document.createElement('div');
        target.id = 'my-target';
        target.setAttribute(
            'v-onclickoutside',
            'window.__elementId = ctx.element.id',
        );
        document.body.appendChild(target);

        const outside = document.createElement('div');
        outside.id = 'outside';
        document.body.appendChild(outside);

        processElement(target, true);

        outside.dispatchEvent(
            new MouseEvent('click', { bubbles: true, cancelable: true }),
        );

        const elementId = (window as unknown as Record<string, string>)
            .__elementId;
        expect(elementId).toBe('my-target');
    });

    it('provides event in context', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-click-outside');

        clearProcessedElements();

        const target = document.createElement('div');
        target.id = 'target';
        target.setAttribute(
            'v-onclickoutside',
            'window.__eventType = ctx.event?.type',
        );
        document.body.appendChild(target);

        const outside = document.createElement('div');
        outside.id = 'outside';
        document.body.appendChild(outside);

        processElement(target, true);

        outside.dispatchEvent(
            new MouseEvent('click', { bubbles: true, cancelable: true }),
        );

        const eventType = (window as unknown as Record<string, string>)
            .__eventType;
        expect(eventType).toBe('click');
    });
});
