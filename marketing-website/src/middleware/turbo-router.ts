// @ts-ignore
import * as Turbo from '@hotwired/turbo'; // eslint-disable-line
import type { TurboEvent } from './turbo-types';

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
    localStorage.setItem(
        'scrollTop',
        document.documentElement.scrollTop.toString(),
    );
});

window.addEventListener('popstate', (e) => {
    if (!e.state?.turbo) return;

    const restore =
        window.Turbo.navigator.history.getRestorationDataForIdentifier(
            e.state.turbo.restorationIdentifier,
        );

    if (restore.scrollPosition) {
        restoreScrollPos(restore.scrollPosition.y);
    }
});

const startTop = localStorage.getItem('scrollTop');

if (startTop) {
    restoreScrollPos(parseInt(startTop));
}

const resizeObserver = new ResizeObserver(() => {
    if (!waitingForScrollSize) return;

    if (document.documentElement.scrollHeight >= waitingForScrollSize) {
        document.documentElement.scrollTop = waitingForScrollSize;
        waitingForScrollSize = undefined;
    }
});

function resetResizeObserver() {
    resizeObserver.disconnect();
    resizeObserver.observe(document.querySelector('main')!);
}

addEventListener('turbo:load', resetResizeObserver);
addEventListener('turbo:render', resetResizeObserver);

Turbo.start();
