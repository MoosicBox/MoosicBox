import { describe, it, expect, beforeEach } from 'vitest';

describe('event', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    it('dispatches custom event from SSE message', async () => {
        await import('../../src/core');
        await import('../../src/event');

        const { triggerMessage } = await import('../../src/core');

        const results: string[] = [];
        window.addEventListener('v-custom-event', ((e: CustomEvent) => {
            results.push(e.detail);
        }) as EventListener);

        // Trigger an event message
        triggerMessage('event', 'custom-event:event-payload');

        expect(results).toContain('event-payload');
    });

    it('parses event name and value from colon-separated format', async () => {
        await import('../../src/core');
        await import('../../src/event');

        const { triggerMessage } = await import('../../src/core');

        const events: Array<{ name: string; value: string }> = [];

        // Listen for multiple event types
        ['v-event-a', 'v-event-b', 'v-event-c'].forEach((name) => {
            window.addEventListener(name, ((e: CustomEvent) => {
                events.push({ name, value: e.detail });
            }) as EventListener);
        });

        triggerMessage('event', 'event-a:value-a');
        triggerMessage('event', 'event-b:value-b');
        triggerMessage('event', 'event-c:value-c');

        expect(events).toHaveLength(3);
        expect(events[0]).toEqual({ name: 'v-event-a', value: 'value-a' });
        expect(events[1]).toEqual({ name: 'v-event-b', value: 'value-b' });
        expect(events[2]).toEqual({ name: 'v-event-c', value: 'value-c' });
    });

    it('handles value containing colons', async () => {
        await import('../../src/core');
        await import('../../src/event');

        const { triggerMessage } = await import('../../src/core');

        let receivedValue = '';
        window.addEventListener('v-time', ((e: CustomEvent) => {
            receivedValue = e.detail;
        }) as EventListener);

        triggerMessage('event', 'time:12:30:45');

        expect(receivedValue).toBe('12:30:45');
    });

    it('ignores messages without colon separator', async () => {
        await import('../../src/core');
        await import('../../src/event');

        const { triggerMessage } = await import('../../src/core');

        let eventFired = false;

        // This would be the event name if parsed incorrectly
        window.addEventListener('v-nocolon', (() => {
            eventFired = true;
        }) as EventListener);

        // This should be ignored (no colon)
        triggerMessage('event', 'nocolon');

        expect(eventFired).toBe(false);
    });

    it('dispatches event on window object', async () => {
        await import('../../src/core');
        await import('../../src/event');

        const { triggerMessage } = await import('../../src/core');

        let target: EventTarget | null = null;
        window.addEventListener('v-test', ((e: Event) => {
            target = e.target;
        }) as EventListener);

        triggerMessage('event', 'test:value');

        expect(target === window ? 'window' : 'other').toBe('window');
    });
});
