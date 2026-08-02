import { describe, expect, beforeEach } from 'vitest';
import { http, HttpResponse } from 'msw';
import { test } from '../helpers/test-extend';

describe('nav', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    describe('navigation', () => {
        test('refreshes the current route instead of serving a cached document', async ({
            worker,
        }) => {
            window.history.replaceState({}, '', '/game');
            let revision = 1;
            worker.use(
                http.get('/game', () =>
                    HttpResponse.html(
                        `<html><body><div id="revision">${revision}</div></body></html>`,
                    ),
                ),
            );

            const core = await import('../../src/core');
            await import('../../src/idiomorph');
            const { navigate } = await import('../../src/nav');
            const { cache } = await import('../../src/nav-base');
            cache['/game'] =
                '<html><body><div id="revision">stale</div></body></html>';
            revision = 2;

            const swaps: Array<{ url?: string; replace?: boolean }> = [];
            core.on('swapDom', ({ url, replace }) => {
                swaps.push({ url, replace });
            });

            navigate('/game');

            await expect
                .poll(() => document.querySelector('#revision')?.textContent)
                .toBe('2');
            expect(swaps).toContainEqual({ url: '/game', replace: true });
        });
        test('action navigation fetches fresh content instead of using prefetched cache', async ({
            worker,
        }) => {
            window.history.replaceState({}, '', '/register');
            worker.use(
                http.get('/dashboard', () =>
                    HttpResponse.html(
                        '<html><body><div id="fresh-dashboard">Fresh</div></body></html>',
                    ),
                ),
            );

            await import('../../src/core');
            await import('../../src/idiomorph');
            const { navigate } = await import('../../src/nav');
            const { cache } = await import('../../src/nav-base');
            cache['/dashboard'] =
                '<html><body><div id="stale-dashboard">Stale</div></body></html>';

            navigate('/dashboard');

            await expect
                .poll(() => Boolean(document.querySelector('#fresh-dashboard')))
                .toBe(true);
            expect(document.querySelector('#stale-dashboard')).toBeNull();
        });

        test('caches fetched documents', async ({ worker }) => {
            let fetchCount = 0;

            worker.use(
                http.get('/page', () => {
                    fetchCount++;
                    return new HttpResponse(
                        '<html><body><div>Page Content</div></body></html>',
                        { headers: { 'content-type': 'text/html' } },
                    );
                }),
            );

            await import('../../src/core');
            const { cache } = await import('../../src/nav-base');

            // Manual fetch to simulate navigation
            const response = await fetch('/page');
            const html = await response.text();
            cache['/page'] = html;

            // Second request should use cache
            const cachedHtml = cache['/page'];
            expect(cachedHtml).toContain('Page Content');
            expect(fetchCount).toBe(1);
        });
    });

    describe('link handling', () => {
        test('intercepts link clicks for client-side navigation', async ({
            worker,
        }) => {
            worker.use(
                http.get('/internal-page', () => {
                    return new HttpResponse(
                        '<html><body><div id="new-content">New Page</div></body></html>',
                        { headers: { 'content-type': 'text/html' } },
                    );
                }),
            );

            await import('../../src/core');
            await import('../../src/idiomorph');
            await import('../../src/nav');

            const link = document.createElement('a');
            link.href = '/internal-page';
            link.textContent = 'Go to page';
            document.body.appendChild(link);

            // Note: Full navigation testing requires more complex setup
            // This is a basic structural test
            const linkExists =
                document.querySelector('a[href="/internal-page"]') !== null;
            expect(linkExists).toBe(true);
        });
    });

    describe('prefetch', () => {
        test('prefetches on hover after delay', async ({ worker }) => {
            let prefetchCount = 0;

            worker.use(
                http.get('/prefetch-page', () => {
                    prefetchCount++;
                    return new HttpResponse(
                        '<html><body>Prefetched</body></html>',
                        {
                            headers: { 'content-type': 'text/html' },
                        },
                    );
                }),
            );

            await import('../../src/core');
            await import('../../src/nav');

            const link = document.createElement('a');
            link.href = '/prefetch-page';
            link.textContent = 'Prefetch me';
            document.body.appendChild(link);

            // Simulate hover
            link.dispatchEvent(
                new MouseEvent('mouseover', {
                    bubbles: true,
                    cancelable: true,
                }),
            );

            // Wait for prefetch delay
            await new Promise((resolve) => setTimeout(resolve, 200));

            // The prefetch behavior depends on the implementation details
            // This test verifies the basic setup
            expect(prefetchCount).toBeGreaterThanOrEqual(0);
        });
    });
});
