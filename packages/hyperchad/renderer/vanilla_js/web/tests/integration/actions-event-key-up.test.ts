import { describe, it, expect, beforeEach, afterEach } from 'vitest';

describe('actions-event-key-up', () => {
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
            window.removeEventListener('v-key-up', listener);
            listener = undefined;
        }
    });

    it('dispatches v-key-up custom event on keyup', async () => {
        await import('../../src/core');
        await import('../../src/actions-event-key-up');

        (window as unknown as Record<string, unknown>).__keyUpEvents = [];

        listener = ((e: CustomEvent) => {
            const arr = (
                window as unknown as Record<
                    string,
                    Array<{ key: string }> | undefined
                >
            ).__keyUpEvents;
            if (arr) arr.push({ key: e.detail });
        }) as EventListener;

        window.addEventListener('v-key-up', listener);

        // Dispatch keyup
        document.dispatchEvent(
            new KeyboardEvent('keyup', { key: 'Enter', bubbles: true }),
        );

        const events = (
            window as unknown as Record<string, Array<{ key: string }>>
        ).__keyUpEvents;
        expect(events).toHaveLength(1);
        expect(events[0].key).toBe('Enter');
    });

    it('dispatches for different keys', async () => {
        await import('../../src/core');
        await import('../../src/actions-event-key-up');

        (window as unknown as Record<string, unknown>).__keys = [];

        listener = ((e: CustomEvent) => {
            const arr = (
                window as unknown as Record<string, string[] | undefined>
            ).__keys;
            if (arr) arr.push(e.detail);
        }) as EventListener;

        window.addEventListener('v-key-up', listener);

        document.dispatchEvent(
            new KeyboardEvent('keyup', { key: 'b', bubbles: true }),
        );
        document.dispatchEvent(
            new KeyboardEvent('keyup', { key: 'Tab', bubbles: true }),
        );
        document.dispatchEvent(
            new KeyboardEvent('keyup', { key: 'ArrowDown', bubbles: true }),
        );

        const keys = (window as unknown as Record<string, string[]>).__keys;
        expect(keys).toEqual(['b', 'Tab', 'ArrowDown']);
    });
});
