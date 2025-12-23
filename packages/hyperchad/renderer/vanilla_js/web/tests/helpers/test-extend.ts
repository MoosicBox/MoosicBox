import { test as testBase, expect } from 'vitest';
import type { SetupWorker } from 'msw/browser';
import { worker } from '../mocks/browser';

interface TestFixtures {
    worker: SetupWorker;
}

export const test = testBase.extend<TestFixtures>({
    worker: [
        async (_, use) => {
            await worker.start({ onUnhandledRequest: 'bypass' });
            await use(worker);
            worker.resetHandlers();
            worker.stop();
        },
        { auto: true },
    ],
});

export { expect };
