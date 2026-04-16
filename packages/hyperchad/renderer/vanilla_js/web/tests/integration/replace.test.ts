import { beforeEach, describe, expect, test, vi } from 'vitest';

describe('replace plugin', () => {
    beforeEach(() => {
        vi.resetModules();
        document.body.innerHTML = '';
    });

    test('swap outer html ignores surrounding whitespace nodes', async () => {
        document.body.innerHTML = `
            <div id="root">
                <h2 id="counter-value">1</h2>
            </div>
        `;

        const errorSpy = vi
            .spyOn(console, 'error')
            .mockImplementation(() => undefined);

        const core = await import('../../src/core');
        await import('../../src/replace');

        core.triggerHandlers('swapHtml', {
            target: '#counter-value',
            html: '\n<h2 id="counter-value">2</h2>\n',
            strategy: 'this',
        });

        const counter = document.querySelector('#counter-value');
        expect(counter?.textContent).toBe('2');
        expect(errorSpy).not.toHaveBeenCalled();

        errorSpy.mockRestore();
    });
});
