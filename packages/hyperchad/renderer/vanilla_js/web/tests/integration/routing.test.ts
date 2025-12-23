import { describe, expect, vi, beforeEach } from 'vitest';
import { http, HttpResponse } from 'msw';
import { test } from '../helpers/test-extend';

describe('routing', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    describe('hx-* attributes', () => {
        test('hx-get triggers GET request on click', async ({ worker }) => {
            let requestReceived = false;

            worker.use(
                http.get('/api/data', () => {
                    requestReceived = true;
                    return new HttpResponse('<div>Response</div>', {
                        headers: { 'content-type': 'text/html' },
                    });
                }),
            );

            await import('../../src/core');
            await import('../../src/routing');

            const btn = document.createElement('button');
            btn.id = 'btn';
            btn.setAttribute('hx-get', '/api/data');
            btn.textContent = 'Load';
            document.body.appendChild(btn);

            btn.dispatchEvent(
                new MouseEvent('click', { bubbles: true, cancelable: true }),
            );

            await vi.waitFor(() => expect(requestReceived).toBe(true), {
                timeout: 3000,
            });
        });

        test('hx-post triggers POST request on click', async ({ worker }) => {
            let requestMethod = '';

            worker.use(
                http.post('/api/submit', () => {
                    requestMethod = 'POST';
                    return new HttpResponse('<div>Submitted</div>', {
                        headers: { 'content-type': 'text/html' },
                    });
                }),
            );

            await import('../../src/core');
            await import('../../src/routing');

            const btn = document.createElement('button');
            btn.id = 'btn';
            btn.setAttribute('hx-post', '/api/submit');
            btn.textContent = 'Submit';
            document.body.appendChild(btn);

            btn.dispatchEvent(
                new MouseEvent('click', { bubbles: true, cancelable: true }),
            );

            await vi.waitFor(() => expect(requestMethod).toBe('POST'), {
                timeout: 3000,
            });
        });

        test('hx-delete triggers DELETE request', async ({ worker }) => {
            let requestMethod = '';

            worker.use(
                http.delete('/api/item/123', () => {
                    requestMethod = 'DELETE';
                    return new HttpResponse(null, { status: 204 });
                }),
            );

            await import('../../src/core');
            await import('../../src/routing');

            const btn = document.createElement('button');
            btn.id = 'btn';
            btn.setAttribute('hx-delete', '/api/item/123');
            btn.textContent = 'Delete';
            document.body.appendChild(btn);

            btn.dispatchEvent(
                new MouseEvent('click', { bubbles: true, cancelable: true }),
            );

            await vi.waitFor(() => expect(requestMethod).toBe('DELETE'), {
                timeout: 3000,
            });
        });
    });

    describe('hx-target', () => {
        test('swaps content into specified target', async ({ worker }) => {
            worker.use(
                http.get('/api/content', () => {
                    return new HttpResponse(
                        '<div id="target"><span>New Content</span></div>',
                        {
                            headers: { 'content-type': 'text/html' },
                        },
                    );
                }),
            );

            await import('../../src/core');
            await import('../../src/idiomorph');
            await import('../../src/routing');

            const target = document.createElement('div');
            target.id = 'target';
            target.innerHTML = '<span>Old Content</span>';
            document.body.appendChild(target);

            const btn = document.createElement('button');
            btn.id = 'btn';
            btn.setAttribute('hx-get', '/api/content');
            btn.setAttribute('hx-target', '#target');
            btn.textContent = 'Load';
            document.body.appendChild(btn);

            btn.dispatchEvent(
                new MouseEvent('click', { bubbles: true, cancelable: true }),
            );

            await vi.waitFor(
                () => {
                    const content =
                        document.getElementById('target')?.textContent;
                    expect(content).toContain('New Content');
                },
                { timeout: 3000 },
            );
        });
    });

    describe('hx-trigger="load"', () => {
        test('triggers request immediately on element processing', async ({
            worker,
        }) => {
            let requestReceived = false;

            worker.use(
                http.get('/api/autoload', () => {
                    requestReceived = true;
                    return new HttpResponse('<div>Auto Loaded</div>', {
                        headers: { 'content-type': 'text/html' },
                    });
                }),
            );

            const { processElement } = await import('../../src/core');
            await import('../../src/routing');

            const div = document.createElement('div');
            div.id = 'autoload';
            div.setAttribute('hx-get', '/api/autoload');
            div.setAttribute('hx-trigger', 'load');
            document.body.appendChild(div);

            processElement(div, true);

            await vi.waitFor(() => expect(requestReceived).toBe(true), {
                timeout: 3000,
            });
        });
    });

    describe('hx-trigger="change"', () => {
        test('triggers request on select change', async ({ worker }) => {
            let requestUrl = '';

            worker.use(
                http.get('/api/select', ({ request }) => {
                    requestUrl = request.url;
                    return new HttpResponse('<div>Updated</div>', {
                        headers: { 'content-type': 'text/html' },
                    });
                }),
            );

            await import('../../src/core');
            await import('../../src/routing');

            const select = document.createElement('select');
            select.id = 'select';
            select.name = 'choice';
            select.setAttribute('hx-get', '/api/select');
            select.setAttribute('hx-trigger', 'change');

            const opt1 = document.createElement('option');
            opt1.value = 'a';
            opt1.textContent = 'A';

            const opt2 = document.createElement('option');
            opt2.value = 'b';
            opt2.textContent = 'B';

            select.appendChild(opt1);
            select.appendChild(opt2);
            document.body.appendChild(select);

            select.value = 'b';
            select.dispatchEvent(new Event('change', { bubbles: true }));

            await vi.waitFor(
                () => {
                    expect(requestUrl).toContain('/api/select');
                    expect(requestUrl).toContain('choice=b');
                },
                { timeout: 3000 },
            );
        });

        test('uses FormData for non-GET requests on change', async ({
            worker,
        }) => {
            let requestBody: FormData | null = null;

            worker.use(
                http.post('/api/update', async ({ request }) => {
                    requestBody = await request.formData();
                    return new HttpResponse('<div>Updated</div>', {
                        headers: { 'content-type': 'text/html' },
                    });
                }),
            );

            await import('../../src/core');
            await import('../../src/routing');

            const input = document.createElement('input');
            input.id = 'input';
            input.name = 'value';
            input.type = 'text';
            input.setAttribute('hx-post', '/api/update');
            input.setAttribute('hx-trigger', 'change');
            document.body.appendChild(input);

            input.value = 'test-value';
            input.dispatchEvent(new Event('change', { bubbles: true }));

            await vi.waitFor(
                () => {
                    expect(requestBody).toBeTruthy();
                    expect(requestBody?.get('value')).toBe('test-value');
                },
                { timeout: 3000 },
            );
        });
    });

    describe('fragment responses', () => {
        test('handles X-HyperChad-Fragments header', async ({ worker }) => {
            worker.use(
                http.get('/api/fragments', () => {
                    const response =
                        '<div>Primary</div>\n<!--hyperchad-fragment-->\n#target\n<div id="target">Fragment Content</div>';
                    return new HttpResponse(response, {
                        headers: {
                            'content-type': 'text/html',
                            'X-HyperChad-Fragments': 'true',
                        },
                    });
                }),
            );

            await import('../../src/core');
            await import('../../src/idiomorph');
            await import('../../src/routing');

            const target = document.createElement('div');
            target.id = 'target';
            target.innerHTML = '<div>Old Fragment</div>';
            document.body.appendChild(target);

            const btn = document.createElement('button');
            btn.id = 'btn';
            btn.setAttribute('hx-get', '/api/fragments');
            btn.textContent = 'Load';
            document.body.appendChild(btn);

            btn.dispatchEvent(
                new MouseEvent('click', { bubbles: true, cancelable: true }),
            );

            await vi.waitFor(
                () => {
                    const content =
                        document.getElementById('target')?.textContent;
                    expect(content).toContain('Fragment Content');
                },
                { timeout: 3000 },
            );
        });
    });

    describe('request headers', () => {
        test('includes hx-request header', async ({ worker }) => {
            let hasHxHeader = false;

            worker.use(
                http.get('/api/check-header', ({ request }) => {
                    hasHxHeader = request.headers.get('hx-request') === 'true';
                    return new HttpResponse('<div>OK</div>', {
                        headers: { 'content-type': 'text/html' },
                    });
                }),
            );

            await import('../../src/core');
            await import('../../src/routing');

            const btn = document.createElement('button');
            btn.id = 'btn';
            btn.setAttribute('hx-get', '/api/check-header');
            btn.textContent = 'Load';
            document.body.appendChild(btn);

            btn.dispatchEvent(
                new MouseEvent('click', { bubbles: true, cancelable: true }),
            );

            await vi.waitFor(() => expect(hasHxHeader).toBe(true), {
                timeout: 3000,
            });
        });
    });
});
