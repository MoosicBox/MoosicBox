import { defineConfig } from 'vitest/config';
import { playwright } from '@vitest/browser-playwright';

export default defineConfig({
    test: {
        projects: [
            {
                test: {
                    name: 'unit',
                    include: ['tests/unit/**/*.test.ts'],
                    environment: 'node',
                },
            },
            {
                test: {
                    name: 'browser',
                    include: ['tests/integration/**/*.test.ts'],
                    setupFiles: ['tests/setup.ts'],
                    browser: {
                        enabled: true,
                        provider: playwright(),
                        instances: [{ browser: 'chromium' }],
                        headless: true,
                        screenshotFailures: false,
                    },
                    testTimeout: 30000,
                },
            },
        ],
    },
});
