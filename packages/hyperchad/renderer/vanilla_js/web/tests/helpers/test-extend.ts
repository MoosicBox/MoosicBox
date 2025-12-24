import { test as testBase, expect } from 'vitest';
import type { SetupWorker } from 'msw/browser';
import { worker } from '../mocks/browser';

interface TestFixtures {
    worker: SetupWorker;
}

export const test = testBase.extend<TestFixtures>({
    worker: [
        // eslint-disable-next-line no-empty-pattern
        async ({}, use) => {
            await worker.start({ onUnhandledRequest: 'bypass' });
            await use(worker);
            worker.resetHandlers();
            worker.stop();
        },
        { auto: true },
    ],
});

export { expect };
