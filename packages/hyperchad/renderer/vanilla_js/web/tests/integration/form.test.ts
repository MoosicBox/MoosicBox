import { describe, expect, vi, beforeEach } from 'vitest';
import { http, HttpResponse } from 'msw';
import { test } from '../helpers/test-extend';

describe('form', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    describe('form submission', () => {
        test('submits form data via fetch', async ({ worker }) => {
            const receivedData: Record<string, string> = {};

            worker.use(
                http.post('/api/form', async ({ request }) => {
                    const formData = await request.formData();
                    formData.forEach((value, key) => {
                        receivedData[key] = value as string;
                    });
                    return new HttpResponse('<div>Success</div>', {
                        headers: { 'content-type': 'text/html' },
                    });
                }),
            );

            await import('../../src/core');
            await import('../../src/form');
            await import('../../src/routing');

            const form = document.createElement('form');
            form.id = 'test-form';
            form.setAttribute('hx-post', '/api/form');

            const input1 = document.createElement('input');
            input1.type = 'text';
            input1.name = 'username';
            input1.value = 'john';

            const input2 = document.createElement('input');
            input2.type = 'email';
            input2.name = 'email';
            input2.value = 'john@example.com';

            const submit = document.createElement('button');
            submit.type = 'submit';
            submit.textContent = 'Submit';

            form.appendChild(input1);
            form.appendChild(input2);
            form.appendChild(submit);
            document.body.appendChild(form);

            form.dispatchEvent(
                new Event('submit', { bubbles: true, cancelable: true }),
            );

            await vi.waitFor(
                () => {
                    expect(receivedData.username).toBe('john');
                    expect(receivedData.email).toBe('john@example.com');
                },
                { timeout: 3000 },
            );
        });

        test('handles checkbox values', async ({ worker }) => {
            const receivedData: Record<string, string> = {};

            worker.use(
                http.post('/api/form', async ({ request }) => {
                    const formData = await request.formData();
                    formData.forEach((value, key) => {
                        receivedData[key] = value as string;
                    });
                    return new HttpResponse('<div>Success</div>', {
                        headers: { 'content-type': 'text/html' },
                    });
                }),
            );

            await import('../../src/core');
            await import('../../src/form');
            await import('../../src/routing');

            const form = document.createElement('form');
            form.id = 'test-form';
            form.setAttribute('hx-post', '/api/form');

            const checkbox = document.createElement('input');
            checkbox.type = 'checkbox';
            checkbox.name = 'agree';
            checkbox.value = 'yes';
            checkbox.checked = true;

            const submit = document.createElement('button');
            submit.type = 'submit';

            form.appendChild(checkbox);
            form.appendChild(submit);
            document.body.appendChild(form);

            form.dispatchEvent(
                new Event('submit', { bubbles: true, cancelable: true }),
            );

            await vi.waitFor(
                () => {
                    expect(receivedData.agree).toBe('yes');
                },
                { timeout: 3000 },
            );
        });

        test('handles select with multiple options', async ({ worker }) => {
            let receivedValues: string[] = [];

            worker.use(
                http.post('/api/form', async ({ request }) => {
                    const formData = await request.formData();
                    receivedValues = formData.getAll('colors') as string[];
                    return new HttpResponse('<div>Success</div>', {
                        headers: { 'content-type': 'text/html' },
                    });
                }),
            );

            await import('../../src/core');
            await import('../../src/form');
            await import('../../src/routing');

            const form = document.createElement('form');
            form.id = 'test-form';
            form.setAttribute('hx-post', '/api/form');

            const select = document.createElement('select');
            select.name = 'colors';
            select.multiple = true;

            const opt1 = document.createElement('option');
            opt1.value = 'red';
            opt1.textContent = 'Red';
            opt1.selected = true;

            const opt2 = document.createElement('option');
            opt2.value = 'blue';
            opt2.textContent = 'Blue';
            opt2.selected = true;

            const opt3 = document.createElement('option');
            opt3.value = 'green';
            opt3.textContent = 'Green';

            select.appendChild(opt1);
            select.appendChild(opt2);
            select.appendChild(opt3);

            const submit = document.createElement('button');
            submit.type = 'submit';

            form.appendChild(select);
            form.appendChild(submit);
            document.body.appendChild(form);

            form.dispatchEvent(
                new Event('submit', { bubbles: true, cancelable: true }),
            );

            await vi.waitFor(
                () => {
                    expect(receivedValues).toContain('red');
                    expect(receivedValues).toContain('blue');
                    expect(receivedValues).not.toContain('green');
                },
                { timeout: 3000 },
            );
        });

        test('handles textarea', async ({ worker }) => {
            const receivedData: Record<string, string> = {};

            worker.use(
                http.post('/api/form', async ({ request }) => {
                    const formData = await request.formData();
                    formData.forEach((value, key) => {
                        receivedData[key] = value as string;
                    });
                    return new HttpResponse('<div>Success</div>', {
                        headers: { 'content-type': 'text/html' },
                    });
                }),
            );

            await import('../../src/core');
            await import('../../src/form');
            await import('../../src/routing');

            const form = document.createElement('form');
            form.id = 'test-form';
            form.setAttribute('hx-post', '/api/form');

            const textarea = document.createElement('textarea');
            textarea.name = 'message';
            textarea.value = 'Hello\nWorld';

            const submit = document.createElement('button');
            submit.type = 'submit';

            form.appendChild(textarea);
            form.appendChild(submit);
            document.body.appendChild(form);

            form.dispatchEvent(
                new Event('submit', { bubbles: true, cancelable: true }),
            );

            await vi.waitFor(
                () => {
                    // Normalize line endings (browser may use \r\n)
                    expect(receivedData.message?.replace(/\r\n/g, '\n')).toBe(
                        'Hello\nWorld',
                    );
                },
                { timeout: 3000 },
            );
        });
    });
});
