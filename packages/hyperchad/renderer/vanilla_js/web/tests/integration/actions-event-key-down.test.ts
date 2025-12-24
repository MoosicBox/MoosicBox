import { describe, it, expect, beforeEach, afterEach } from 'vitest';

describe('actions-event-key-down', () => {
    let listener: EventListener | undefined;

    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    afterEach(() => {
        if (listener) {
            window.removeEventListener('v-key-down', listener);
            listener = undefined;
        }
    });

    it('dispatches v-key-down custom event on keydown', async () => {
        await import('../../src/core');
        await import('../../src/actions-event-key-down');

        (window as unknown as Record<string, unknown>).__keyDownEvents = [];

        listener = ((e: CustomEvent) => {
            const arr = (
                window as unknown as Record<
                    string,
                    Array<{ key: string }> | undefined
                >
            ).__keyDownEvents;
            if (arr) arr.push({ key: e.detail });
        }) as EventListener;

        window.addEventListener('v-key-down', listener);

        // Dispatch keydown
        document.dispatchEvent(
            new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }),
        );

        const events = (
            window as unknown as Record<string, Array<{ key: string }>>
        ).__keyDownEvents;
        expect(events).toHaveLength(1);
        expect(events[0].key).toBe('Enter');
    });

    it('dispatches for different keys', async () => {
        await import('../../src/core');
        await import('../../src/actions-event-key-down');

        (window as unknown as Record<string, unknown>).__keys = [];

        listener = ((e: CustomEvent) => {
            const arr = (
                window as unknown as Record<string, string[] | undefined>
            ).__keys;
            if (arr) arr.push(e.detail);
        }) as EventListener;

        window.addEventListener('v-key-down', listener);

        document.dispatchEvent(
            new KeyboardEvent('keydown', { key: 'a', bubbles: true }),
        );
        document.dispatchEvent(
            new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
        );
        document.dispatchEvent(
            new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }),
        );

        const keys = (window as unknown as Record<string, string[]>).__keys;
        expect(keys).toEqual(['a', 'Escape', 'ArrowUp']);
    });
});
