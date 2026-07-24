import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
    captureReadingPosition,
    restoreReadingPosition,
} from '../../src/reading-position';

function setScrollerMetrics(
    scrollTop: number,
    scrollHeight: number,
    clientHeight: number,
): HTMLElement {
    const scroller = document.documentElement;
    Object.defineProperties(scroller, {
        scrollTop: { configurable: true, value: scrollTop, writable: true },
        scrollHeight: { configurable: true, value: scrollHeight },
        clientHeight: { configurable: true, value: clientHeight },
    });
    Object.defineProperty(document, 'scrollingElement', {
        configurable: true,
        value: scroller,
    });
    return scroller;
}

describe('reading position', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        Object.defineProperty(window, 'innerHeight', {
            configurable: true,
            value: 800,
        });
    });

    it('keeps a reader at the end when streamed content grows', () => {
        const target = document.createElement('main');
        document.body.appendChild(target);
        const scroller = setScrollerMetrics(1200, 2000, 800);

        const snapshot = captureReadingPosition(target);
        Object.defineProperty(scroller, 'scrollHeight', {
            configurable: true,
            value: 2400,
        });
        restoreReadingPosition(snapshot);

        expect(scroller.scrollTop).toBe(1600);
    });

    it('preserves the visible anchor while older content is inserted', () => {
        const target = document.createElement('main');
        const anchor = document.createElement('article');
        anchor.id = 'transcript-item-42';
        target.appendChild(anchor);
        document.body.appendChild(target);
        const scroller = setScrollerMetrics(500, 3000, 800);
        vi.spyOn(anchor, 'getBoundingClientRect').mockReturnValue({
            top: 120,
            bottom: 320,
        } as DOMRect);

        const snapshot = captureReadingPosition(target);
        vi.mocked(anchor.getBoundingClientRect).mockReturnValue({
            top: 420,
            bottom: 620,
        } as DOMRect);
        restoreReadingPosition(snapshot);

        expect(scroller.scrollTop).toBe(800);
    });

    it('falls back to the prior offset when an anchor disappears', () => {
        const target = document.createElement('main');
        const anchor = document.createElement('article');
        anchor.id = 'transcript-item-42';
        target.appendChild(anchor);
        document.body.appendChild(target);
        const scroller = setScrollerMetrics(500, 3000, 800);
        vi.spyOn(anchor, 'getBoundingClientRect').mockReturnValue({
            top: 120,
            bottom: 320,
        } as DOMRect);

        const snapshot = captureReadingPosition(target);
        anchor.remove();
        scroller.scrollTop = 900;
        restoreReadingPosition(snapshot);

        expect(scroller.scrollTop).toBe(500);
    });
});
