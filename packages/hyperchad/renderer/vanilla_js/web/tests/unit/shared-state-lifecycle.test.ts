import { describe, expect, test } from 'vitest';
import { streamErrorLifecycle, streamStartLifecycle } from '../../src/shared-state-lifecycle';

describe('shared-state lifecycle', () => {
    test('reports initial connection and reconnect attempts', () => {
        expect(streamStartLifecycle(false)).toBe('shared-state-connecting');
        expect(streamStartLifecycle(true)).toBe('shared-state-reconnecting');
    });

    test('reports disconnects after a live stream and reconnecting before one', () => {
        expect(streamErrorLifecycle(true)).toBe('shared-state-disconnected');
        expect(streamErrorLifecycle(false)).toBe('shared-state-reconnecting');
    });
});
