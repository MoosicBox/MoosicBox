// @ts-ignore
import * as Turbo from '@hotwired/turbo'; // eslint-disable-line
import type { TurboEvent, TurboVisitEvent } from './turbo-types';

function defaultEventHandler(_event: TurboEvent) {}

addEventListener('turbo:click', defaultEventHandler);
addEventListener('turbo:before-visit', defaultEventHandler);
addEventListener('turbo:visit', defaultEventHandler);
addEventListener('turbo:before-cache', defaultEventHandler);
addEventListener('turbo:before-render', defaultEventHandler);
addEventListener('turbo:render', defaultEventHandler);
addEventListener('turbo:load', defaultEventHandler);
addEventListener('turbo:morph', defaultEventHandler);
addEventListener('turbo:before-morph-element', defaultEventHandler);
addEventListener('turbo:before-morph-attribute', defaultEventHandler);
addEventListener('turbo:morph-element', defaultEventHandler);
addEventListener('turbo:submit-start', defaultEventHandler);
addEventListener('turbo:submit-end', defaultEventHandler);
addEventListener('turbo:before-frame-render', defaultEventHandler);
addEventListener('turbo:frame-render', defaultEventHandler);
addEventListener('turbo:frame-load', defaultEventHandler);
addEventListener('turbo:frame-missing', defaultEventHandler);
addEventListener('turbo:before-stream-render', defaultEventHandler);
addEventListener('turbo:before-fetch-request', defaultEventHandler);
addEventListener('turbo:before-fetch-response', defaultEventHandler);
addEventListener('turbo:before-prefetch', defaultEventHandler);
addEventListener('turbo:fetch-request-error', defaultEventHandler);

let waitingForScrollSize: number | undefined;

function restoreScrollPos(position: number) {
    waitingForScrollSize = position;
}

window.addEventListener('beforeunload', () => {
    const main = document.querySelector('main');
    if (main) {
        scrollTops['refresh'] = main.scrollTop;
    }
    localStorage.setItem('scrollTops', JSON.stringify(scrollTops));
});

const scrollTopsJson = localStorage.getItem('scrollTops');
const scrollTops: { [restorationId: string]: number } = scrollTopsJson
    ? JSON.parse(scrollTopsJson)
    : {};

if (scrollTops['refresh']) {
    const pos = scrollTops['refresh'];
    delete scrollTops['refresh'];
    restoreScrollPos(pos);
}

document.addEventListener('turbo:visit', (event: TurboVisitEvent) => {
    switch (event.detail?.action) {
        case 'restore': {
            const main = document.querySelector('main');

            if (main) {
                const restorationId =
                    Turbo.navigator.history.restorationIdentifier;
                restoreScrollPos(scrollTops[restorationId] ?? 0);
            }
            break;
        }
        case 'advance':
        case 'replace': {
            const main = document.querySelector('main');

            if (main) {
                const restorationId =
                    Turbo.navigator.history.restorationIdentifier;
                scrollTops[restorationId] = main.scrollTop;
            }
            break;
        }
    }
});

const resizeObserver = new ResizeObserver(() => {
    if (!waitingForScrollSize) return;

    const main = document.querySelector('main');

    if (main) {
        if (main.scrollHeight >= waitingForScrollSize) {
            trySetScrollTop(main, waitingForScrollSize);
            waitingForScrollSize = undefined;
        }
    }
});

function trySetScrollTop(
    element: HTMLElement,
    pos: number,
    attempt: number = 0,
) {
    if (attempt > 20) return;

    element.scrollTop = pos;

    if (element.scrollTop === pos) return;

    setTimeout(() => trySetScrollTop(element, pos, attempt + 1), 10);
}

function resetResizeObserver() {
    resizeObserver.disconnect();
    resizeObserver.observe(document.querySelector('main')!);
}

addEventListener('turbo:load', resetResizeObserver);
addEventListener('turbo:render', resetResizeObserver);

Turbo.start();
