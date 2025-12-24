import { describe, it, expect, beforeEach } from 'vitest';

describe('actions-mouse-over', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    it('triggers on mouseover with v-onmouseover attribute', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-mouse-over');

        clearProcessedElements();

        const div = document.createElement('div');
        div.id = 'hover-target';
        div.setAttribute('v-onmouseover', 'window.__hovered = true');
        div.textContent = 'Hover me';
        document.body.appendChild(div);

        processElement(div, true);

        div.dispatchEvent(
            new MouseEvent('mouseenter', { bubbles: true, cancelable: true }),
        );

        const hovered = (window as unknown as Record<string, boolean>)
            .__hovered;
        expect(hovered).toBe(true);
    });

    it('provides element in context', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-mouse-over');

        clearProcessedElements();

        const div = document.createElement('div');
        div.id = 'my-element';
        div.setAttribute(
            'v-onmouseover',
            'window.__elementId = ctx.element.id',
        );
        div.textContent = 'Hover me';
        document.body.appendChild(div);

        processElement(div, true);

        div.dispatchEvent(
            new MouseEvent('mouseenter', { bubbles: true, cancelable: true }),
        );

        const elementId = (window as unknown as Record<string, string>)
            .__elementId;
        expect(elementId).toBe('my-element');
    });

    it('provides event in context', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-mouse-over');

        clearProcessedElements();

        const div = document.createElement('div');
        div.id = 'div';
        div.setAttribute(
            'v-onmouseover',
            'window.__eventType = ctx.event?.type',
        );
        div.textContent = 'Hover me';
        document.body.appendChild(div);

        processElement(div, true);

        div.dispatchEvent(
            new MouseEvent('mouseenter', { bubbles: true, cancelable: true }),
        );

        const eventType = (window as unknown as Record<string, string>)
            .__eventType;
        expect(eventType).toBe('mouseenter');
    });

    it('bubbles up to find v-onmouseover attribute', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-mouse-over');

        clearProcessedElements();

        const parent = document.createElement('div');
        parent.id = 'parent';
        parent.setAttribute('v-onmouseover', 'window.__parentHovered = true');

        const child = document.createElement('span');
        child.id = 'child';
        child.textContent = 'Hover me';

        parent.appendChild(child);
        document.body.appendChild(parent);

        processElement(parent, true);

        child.dispatchEvent(
            new MouseEvent('mouseenter', { bubbles: true, cancelable: true }),
        );

        const parentHovered = (window as unknown as Record<string, boolean>)
            .__parentHovered;
        expect(parentHovered).toBe(true);
    });

    it('handles mouseout to reset state', async () => {
        const { processElement, clearProcessedElements } =
            await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-mouse-over');

        clearProcessedElements();

        (window as unknown as Record<string, unknown>).__hoverState = 'none';

        const div = document.createElement('div');
        div.id = 'div';
        div.setAttribute(
            'v-onmouseover',
            "window.__hoverState = window.__hoverState === 'none' ? 'entered' : 'reset'",
        );
        div.textContent = 'Hover me';
        document.body.appendChild(div);

        processElement(div, true);

        // First hover
        div.dispatchEvent(
            new MouseEvent('mouseenter', { bubbles: true, cancelable: true }),
        );

        let hoverState = (window as unknown as Record<string, string>)
            .__hoverState;
        expect(hoverState).toBe('entered');

        // Mouse out
        div.dispatchEvent(
            new MouseEvent('mouseleave', { bubbles: true, cancelable: true }),
        );

        // Second hover
        div.dispatchEvent(
            new MouseEvent('mouseenter', { bubbles: true, cancelable: true }),
        );

        hoverState = (window as unknown as Record<string, string>).__hoverState;
        expect(hoverState).toBe('reset');
    });
});
