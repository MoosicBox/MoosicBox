import { describe, it, expect, beforeEach } from 'vitest';

describe('core', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    describe('event system', () => {
        describe('on() and triggerHandlers()', () => {
            it('registers and triggers event handlers', async () => {
                const { on, triggerHandlers } = await import('../../src/core');

                const results: string[] = [];
                on('domLoad', (payload) => {
                    results.push(`domLoad:${payload.initial}`);
                });

                triggerHandlers('domLoad', {
                    initial: true,
                    navigation: false,
                    elements: [],
                });

                expect(results).toContain('domLoad:true');
            });

            it('supports multiple handlers for same event', async () => {
                const { on, triggerHandlers } = await import('../../src/core');

                const results: number[] = [];
                on('swapDom', () => results.push(1));
                on('swapDom', () => results.push(2));

                triggerHandlers('swapDom', { html: '<div></div>' });

                expect(results).toEqual([1, 2]);
            });
        });

        describe('onMessage() and triggerMessage()', () => {
            it('registers and triggers message handlers', async () => {
                const { onMessage, triggerMessage } =
                    await import('../../src/core');

                const results: Array<{ data: string; id?: string }> = [];
                onMessage('view', (data, id) => {
                    results.push({ data, id });
                });

                triggerMessage('view', '<div>Hello</div>', 'msg-123');

                expect(results).toHaveLength(1);
                expect(results[0].data).toBe('<div>Hello</div>');
                expect(results[0].id).toBe('msg-123');
            });

            it('ignores messages for unregistered types', async () => {
                const { triggerMessage } = await import('../../src/core');

                // Should not throw
                triggerMessage('unknown_type', 'data');

                expect(true).toBe(true);
            });
        });

        describe('onAttr() and onAttrValue()', () => {
            it('calls handler when element has matching attribute', async () => {
                const { onAttr, processElement } =
                    await import('../../src/core');

                const results: string[] = [];
                onAttr('v-test', ({ element, attr }) => {
                    results.push(`${element.id}:${attr}`);
                });

                const el = document.createElement('div');
                el.id = 'test-el';
                el.setAttribute('v-test', 'test-value');
                document.body.appendChild(el);

                processElement(el, true);

                expect(results).toContain('test-el:test-value');
            });

            it('onAttrValue only triggers for specific value', async () => {
                const { onAttrValue, processElement } =
                    await import('../../src/core');

                const results: string[] = [];
                onAttrValue('hx-trigger', 'load', ({ element }) => {
                    results.push(element.id);
                });

                const el1 = document.createElement('div');
                el1.id = 'el1';
                el1.setAttribute('hx-trigger', 'load');

                const el2 = document.createElement('div');
                el2.id = 'el2';
                el2.setAttribute('hx-trigger', 'click');

                document.body.appendChild(el1);
                document.body.appendChild(el2);

                processElement(el1, true);
                processElement(el2, true);

                expect(results).toEqual(['el1']); // Only el1 has hx-trigger="load"
            });
        });
    });

    describe('processElement()', () => {
        it('processes element and its children', async () => {
            const { onElement, processElement, clearProcessedElements } =
                await import('../../src/core');

            clearProcessedElements();

            const processed: string[] = [];
            onElement(({ element }) => {
                if (element.id) processed.push(element.id);
            });

            const parent = document.createElement('div');
            parent.id = 'parent';

            const child = document.createElement('span');
            child.id = 'child';

            parent.appendChild(child);
            document.body.appendChild(parent);

            processElement(parent, true);

            expect(processed).toContain('parent');
            expect(processed).toContain('child');
        });

        it('skips already processed elements unless force is true', async () => {
            const { onElement, processElement, clearProcessedElements } =
                await import('../../src/core');

            clearProcessedElements();

            let processCount = 0;
            onElement(() => {
                processCount++;
            });

            const el = document.createElement('div');
            el.id = 'test';
            document.body.appendChild(el);

            processElement(el);
            processElement(el); // Should be skipped
            processElement(el, true); // Should process (force)

            expect(processCount).toBe(2); // First call + force call
        });
    });

    describe('DOM utilities', () => {
        describe('splitHtml()', () => {
            it('returns html only when no style present', async () => {
                const { splitHtml } = await import('../../src/core');

                const result = splitHtml('<div>Hello</div>');

                expect(result.html).toBe('<div>Hello</div>');
                expect(result.style).toBeUndefined();
            });

            it('separates style and html when both present', async () => {
                const { splitHtml } = await import('../../src/core');

                const result = splitHtml(
                    '<style>.test{color:red}</style>\n\n<div>Hello</div>',
                );

                expect(result.style).toBe('<style>.test{color:red}</style>');
                expect(result.html).toBe('<div>Hello</div>');
            });
        });

        describe('decodeHtml()', () => {
            it('decodes HTML entities', async () => {
                const { decodeHtml } = await import('../../src/core');

                const result = decodeHtml(
                    '&lt;div&gt;Hello &amp; World&lt;/div&gt;',
                );

                expect(result).toBe('<div>Hello & World</div>');
            });
        });

        describe('htmlToStyle()', () => {
            it('creates style element with v-id attribute', async () => {
                const { htmlToStyle } = await import('../../src/core');

                const style = htmlToStyle(
                    '<style>.test{color:red}</style>',
                    'trigger-123',
                );

                expect(style?.tagName).toBe('STYLE');
                expect(style?.getAttribute('v-id')).toBe('trigger-123');
            });
        });

        describe('removeElementStyles()', () => {
            it('removes all styles with matching v-id', async () => {
                const { removeElementStyles } = await import('../../src/core');

                // Add some style elements
                const style1 = document.createElement('style');
                style1.setAttribute('v-id', 'trigger-123');
                document.head.appendChild(style1);

                const style2 = document.createElement('style');
                style2.setAttribute('v-id', 'trigger-123');
                document.head.appendChild(style2);

                const style3 = document.createElement('style');
                style3.setAttribute('v-id', 'other');
                document.head.appendChild(style3);

                removeElementStyles('trigger-123');

                const remaining =
                    document.querySelectorAll('style[v-id]').length;
                const hasOther =
                    document.querySelector('style[v-id="other"]') !== null;

                expect(remaining).toBe(1);
                expect(hasOther).toBe(true);
            });
        });
    });

    describe('createEventDelegator()', () => {
        it('delegates events by bubbling to find attribute', async () => {
            const { createEventDelegator } = await import('../../src/core');

            const results: Array<{ elementId: string; attr: string }> = [];

            createEventDelegator('click', 'v-test-click', (element, attr) => {
                results.push({ elementId: element.id, attr });
            });

            const parent = document.createElement('div');
            parent.id = 'parent';
            parent.setAttribute('v-test-click', 'parent-action');

            const child = document.createElement('span');
            child.id = 'child';
            child.textContent = 'Click me';

            parent.appendChild(child);
            document.body.appendChild(parent);

            // Dispatch click on child
            child.dispatchEvent(
                new MouseEvent('click', { bubbles: true, cancelable: true }),
            );

            expect(results).toHaveLength(1);
            expect(results[0].elementId).toBe('parent');
            expect(results[0].attr).toBe('parent-action');
        });

        it('stops at first element with attribute', async () => {
            const { createEventDelegator } = await import('../../src/core');

            const results: string[] = [];

            createEventDelegator('click', 'v-test-nested', (element) => {
                results.push(element.id);
            });

            const outer = document.createElement('div');
            outer.id = 'outer';
            outer.setAttribute('v-test-nested', 'outer-action');

            const inner = document.createElement('div');
            inner.id = 'inner';
            inner.setAttribute('v-test-nested', 'inner-action');

            outer.appendChild(inner);
            document.body.appendChild(outer);

            // Click on inner - should only find inner, not bubble to outer
            inner.dispatchEvent(
                new MouseEvent('click', { bubbles: true, cancelable: true }),
            );

            expect(results).toEqual(['inner']);
        });
    });

    describe('message handlers for SSE events', () => {
        it('view message triggers swapDom', async () => {
            const { triggerMessage, on } = await import('../../src/core');

            const results: Array<{ html: string }> = [];
            on('swapDom', (payload) => {
                results.push({ html: payload.html as string });
            });

            triggerMessage('view', '<div>New Content</div>');

            expect(results).toHaveLength(1);
            expect(results[0].html).toBe('<div>New Content</div>');
        });

        it('partial_view message triggers swapHtml with target', async () => {
            const { triggerMessage, on } = await import('../../src/core');

            const results: Array<{
                target: string;
                html: string;
                strategy: string;
            }> = [];
            on('swapHtml', (payload) => {
                results.push({
                    target: payload.target as string,
                    html: payload.html as string,
                    strategy: payload.strategy,
                });
            });

            triggerMessage(
                'partial_view',
                '<style>.x{}</style>\n\n<div>Updated</div>',
                'element-123',
            );

            expect(results).toHaveLength(1);
            expect(results[0].target).toBe('#element-123');
            expect(results[0].html).toBe('<div>Updated</div>');
            expect(results[0].strategy).toBe('this');
        });
    });
});
