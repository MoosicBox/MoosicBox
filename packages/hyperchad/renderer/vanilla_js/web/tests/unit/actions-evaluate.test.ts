/**
 * Unit tests for the actions module.
 *
 * Note: The evaluate() function is heavily DOM-dependent and uses eval(),
 * making it better suited for browser integration tests. These unit tests
 * focus on the triggerAction function which is more testable in isolation.
 *
 * See tests/integration/actions.test.ts for comprehensive evaluate() tests.
 */

// @vitest-environment jsdom

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

describe('triggerAction', () => {
    let originalFetch: typeof fetch;

    beforeEach(() => {
        originalFetch = globalThis.fetch;
        globalThis.fetch = vi.fn().mockResolvedValue(new Response());
    });

    afterEach(() => {
        globalThis.fetch = originalFetch;
    });

    it('sends POST request to $action endpoint', async () => {
        const { triggerAction } = await import('../../src/actions');

        triggerAction({ action: 'test-action' });

        expect(globalThis.fetch).toHaveBeenCalledWith('$action', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({ action: 'test-action' }),
        });
    });

    it('includes value in request body when provided', async () => {
        const { triggerAction } = await import('../../src/actions');

        triggerAction({ action: 'test-action', value: { foo: 'bar' } });

        expect(globalThis.fetch).toHaveBeenCalledWith('$action', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({
                action: 'test-action',
                value: { foo: 'bar' },
            }),
        });
    });

    it('serializes complex action objects', async () => {
        const { triggerAction } = await import('../../src/actions');

        const complexAction = {
            action: {
                type: 'navigate',
                params: { page: 1, filter: 'active' },
            },
            value: ['a', 'b', 'c'],
        };

        triggerAction(complexAction);

        expect(globalThis.fetch).toHaveBeenCalledWith('$action', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify(complexAction),
        });
    });
});
