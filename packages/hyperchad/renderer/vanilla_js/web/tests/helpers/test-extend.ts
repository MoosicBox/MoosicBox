import { test as testBase, expect } from 'vitest';
import type { SetupWorker } from 'msw/browser';
import { worker } from '../mocks/browser';

interface TestFixtures {
    worker: SetupWorker;
}

let workerStarted = false;

export const test = testBase.extend<TestFixtures>({
    worker: [
        // eslint-disable-next-line no-empty-pattern
        async ({}, use) => {
            if (!workerStarted) {
                await worker.start({ onUnhandledRequest: 'bypass' });
                workerStarted = true;
            }

            await use(worker);

            const { stopAllEventSourceStreams } = await import(
                '../../src/sse-base'
            );

            stopAllEventSourceStreams();
            worker.resetHandlers();
        },
        { auto: true },
    ],
});

export { expect };
