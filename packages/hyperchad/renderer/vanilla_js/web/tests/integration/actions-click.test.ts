import { describe, it, expect, beforeEach } from 'vitest';

describe('actions-click', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    it('triggers evaluate on click with v-onclick attribute', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-click');

        const btn = document.createElement('button');
        btn.id = 'btn';
        btn.setAttribute('v-onclick', 'window.__clicked = true');
        btn.textContent = 'Click me';
        document.body.appendChild(btn);

        // Click the button
        btn.dispatchEvent(
            new MouseEvent('click', { bubbles: true, cancelable: true }),
        );

        const clicked = (window as unknown as Record<string, boolean>)
            .__clicked;
        expect(clicked).toBe(true);
    });

    it('stops event propagation', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-click');

        (window as unknown as Record<string, boolean>).__parentClicked = false;
        (window as unknown as Record<string, boolean>).__childClicked = false;

        const parent = document.createElement('div');
        parent.id = 'parent';
        parent.addEventListener('click', () => {
            (window as unknown as Record<string, boolean>).__parentClicked =
                true;
        });

        const child = document.createElement('button');
        child.id = 'child';
        child.setAttribute('v-onclick', 'window.__childClicked = true');
        child.textContent = 'Click me';

        parent.appendChild(child);
        document.body.appendChild(parent);

        child.dispatchEvent(
            new MouseEvent('click', { bubbles: true, cancelable: true }),
        );

        const childClicked = (window as unknown as Record<string, boolean>)
            .__childClicked;
        const parentClicked = (window as unknown as Record<string, boolean>)
            .__parentClicked;

        expect(childClicked).toBe(true);
        expect(parentClicked).toBe(false); // Propagation stopped
    });

    it('bubbles up DOM tree to find v-onclick attribute', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-click');

        const parent = document.createElement('div');
        parent.id = 'parent';
        parent.setAttribute('v-onclick', 'window.__clicked = true');

        const child = document.createElement('span');
        child.id = 'child';
        child.textContent = 'Click me';

        parent.appendChild(child);
        document.body.appendChild(parent);

        child.dispatchEvent(
            new MouseEvent('click', { bubbles: true, cancelable: true }),
        );

        const clicked = (window as unknown as Record<string, boolean>)
            .__clicked;
        expect(clicked).toBe(true);
    });

    it('handles URI-encoded action values', async () => {
        const encodedAction = encodeURIComponent(
            'window.__value = "hello world"',
        );

        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-click');

        const btn = document.createElement('button');
        btn.id = 'btn';
        btn.setAttribute('v-onclick', encodedAction);
        btn.textContent = 'Click me';
        document.body.appendChild(btn);

        btn.dispatchEvent(
            new MouseEvent('click', { bubbles: true, cancelable: true }),
        );

        const value = (window as unknown as Record<string, string>).__value;
        expect(value).toBe('hello world');
    });

    it('provides element in context', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-click');

        const btn = document.createElement('button');
        btn.id = 'test-btn';
        btn.setAttribute('v-onclick', 'window.__elementId = ctx.element.id');
        btn.textContent = 'Click me';
        document.body.appendChild(btn);

        btn.dispatchEvent(
            new MouseEvent('click', { bubbles: true, cancelable: true }),
        );

        const elementId = (window as unknown as Record<string, string>)
            .__elementId;
        expect(elementId).toBe('test-btn');
    });

    it('provides event in context', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-click');

        const btn = document.createElement('button');
        btn.id = 'btn';
        btn.setAttribute('v-onclick', 'window.__eventType = ctx.event?.type');
        btn.textContent = 'Click me';
        document.body.appendChild(btn);

        btn.dispatchEvent(
            new MouseEvent('click', { bubbles: true, cancelable: true }),
        );

        const eventType = (window as unknown as Record<string, string>)
            .__eventType;
        expect(eventType).toBe('click');
    });

    it('handles errors gracefully without crashing', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-click');

        // Track console errors
        const errors: string[] = [];
        const originalError = console.error;
        console.error = (...args: unknown[]) => {
            errors.push(args.join(' '));
            originalError.apply(console, args);
        };

        const btn = document.createElement('button');
        btn.id = 'btn';
        btn.setAttribute('v-onclick', 'throw new Error("intentional error")');
        btn.textContent = 'Click me';
        document.body.appendChild(btn);

        // Should not throw, error is caught
        btn.dispatchEvent(
            new MouseEvent('click', { bubbles: true, cancelable: true }),
        );

        // Restore console.error
        console.error = originalError;

        expect(
            errors.some((e) => e.includes('onclick') || e.includes('failed')),
        ).toBe(true);
    });
});
