export interface ReadingPositionSnapshot {
    scrollTop: number;
    atEnd: boolean;
    anchorId?: string;
    anchorTop?: number;
}

const END_THRESHOLD_PX = 48;

function scrollingElement(): HTMLElement | undefined {
    return document.scrollingElement instanceof HTMLElement
        ? document.scrollingElement
        : document.documentElement;
}

function visibleAnchor(target: HTMLElement): HTMLElement | undefined {
    const viewportHeight = window.innerHeight;
    return Array.from(target.querySelectorAll<HTMLElement>('[id]')).find(
        (element) => {
            const bounds = element.getBoundingClientRect();
            return bounds.bottom > 0 && bounds.top < viewportHeight;
        },
    );
}

export function captureReadingPosition(
    target: HTMLElement,
): ReadingPositionSnapshot | undefined {
    const scroller = scrollingElement();
    if (!scroller) return undefined;

    const maxScrollTop = Math.max(
        0,
        scroller.scrollHeight - scroller.clientHeight,
    );
    const anchor = visibleAnchor(target);
    return {
        scrollTop: scroller.scrollTop,
        atEnd: maxScrollTop - scroller.scrollTop <= END_THRESHOLD_PX,
        anchorId: anchor?.id,
        anchorTop: anchor?.getBoundingClientRect().top,
    };
}

export function restoreReadingPosition(
    snapshot: ReadingPositionSnapshot | undefined,
): void {
    if (!snapshot) return;

    const scroller = scrollingElement();
    if (!scroller) return;

    if (snapshot.atEnd) {
        scroller.scrollTop = Math.max(
            0,
            scroller.scrollHeight - scroller.clientHeight,
        );
        return;
    }

    if (snapshot.anchorId && snapshot.anchorTop !== undefined) {
        const anchor = document.getElementById(snapshot.anchorId);
        if (anchor) {
            scroller.scrollTop +=
                anchor.getBoundingClientRect().top - snapshot.anchorTop;
            return;
        }
    }

    scroller.scrollTop = snapshot.scrollTop;
}
