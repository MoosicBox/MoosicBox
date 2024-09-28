import './scan-status-banner.css';
import { showScanStatusBanner } from '~/services/app';
import { clientSignal, deepEqual } from '~/services/util';
import { hideTask, scanState } from '~/services/scan';
import { For, Show } from 'solid-js';

const responsePromiseResolves: ((yes: boolean) => void)[] = [];

export async function responsePromise(): Promise<boolean> {
    return new Promise((resolve) => {
        responsePromiseResolves.push(resolve);
    });
}

export default function scanStatusBannerFunc() {
    const [$showScanStatusBanner] = clientSignal(showScanStatusBanner);

    return (
        <div data-turbo-permanent id="scan-status-banner">
            <Show when={$showScanStatusBanner()}>
                <For each={scanState.tasks}>
                    {(task) => (
                        <Show
                            when={scanState.hiddenTasks.every(
                                (x) => !deepEqual(x, task.task),
                            )}
                        >
                            <div class="scan-status-banner-scan-task">
                                {task.scanned.toLocaleString()} of{' '}
                                {task.total.toLocaleString()} track
                                {task.total === 1 ? '' : 's'} scanned
                                <button
                                    class="remove-button-styles"
                                    onClick={() => hideTask(task.task)}
                                >
                                    <img
                                        class="cross-icon"
                                        src="/img/cross-white.svg"
                                        alt="Dismiss banner"
                                    />
                                </button>
                            </div>
                        </Show>
                    )}
                </For>
            </Show>
        </div>
    );
}
