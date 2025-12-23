import { describe, it, expect, beforeEach } from 'vitest';

describe('idiomorph', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    describe('DOM morphing', () => {
        it('morphs DOM on swapHtml event with strategy "this"', async () => {
            await import('../../src/core');
            await import('../../src/idiomorph');

            const { triggerHandlers } = await import('../../src/core');

            const target = document.createElement('div');
            target.id = 'target';
            target.innerHTML = '<span>Old Content</span>';
            document.body.appendChild(target);

            triggerHandlers('swapHtml', {
                target: '#target',
                html: '<div id="target"><span>New Content</span></div>',
                strategy: 'this',
            });

            const content = document.getElementById('target')?.textContent;
            expect(content).toBe('New Content');
        });

        it('morphs DOM with strategy "children"', async () => {
            await import('../../src/core');
            await import('../../src/idiomorph');

            const { triggerHandlers } = await import('../../src/core');

            const target = document.createElement('div');
            target.id = 'target';
            target.innerHTML = '<span>Old</span>';
            document.body.appendChild(target);

            triggerHandlers('swapHtml', {
                target: '#target',
                html: '<span>New Children</span>',
                strategy: 'children',
            });

            const content = document.getElementById('target')?.innerHTML;
            expect(content).toContain('New Children');
        });

        it('handles beforebegin strategy', async () => {
            await import('../../src/core');
            await import('../../src/idiomorph');

            const { triggerHandlers } = await import('../../src/core');

            const target = document.createElement('div');
            target.id = 'target';
            target.textContent = 'Target';
            document.body.appendChild(target);

            triggerHandlers('swapHtml', {
                target: '#target',
                html: '<span id="before">Before</span>',
                strategy: 'beforebegin',
            });

            const beforeElement =
                document.getElementById('before')?.textContent;
            expect(beforeElement).toBe('Before');

            // Verify order
            const children = Array.from(document.body.children);
            const order = children.map((c) => c.id);
            expect(order.indexOf('before')).toBeLessThan(
                order.indexOf('target'),
            );
        });

        it('handles afterend strategy', async () => {
            await import('../../src/core');
            await import('../../src/idiomorph');

            const { triggerHandlers } = await import('../../src/core');

            const target = document.createElement('div');
            target.id = 'target';
            target.textContent = 'Target';
            document.body.appendChild(target);

            triggerHandlers('swapHtml', {
                target: '#target',
                html: '<span id="after">After</span>',
                strategy: 'afterend',
            });

            const afterElement = document.getElementById('after')?.textContent;
            expect(afterElement).toBe('After');

            // Verify order
            const children = Array.from(document.body.children);
            const order = children.map((c) => c.id);
            expect(order.indexOf('target')).toBeLessThan(
                order.indexOf('after'),
            );
        });

        it('handles delete strategy', async () => {
            await import('../../src/core');
            await import('../../src/idiomorph');

            const { triggerHandlers } = await import('../../src/core');

            const target = document.createElement('div');
            target.id = 'target';
            target.textContent = 'To be deleted';
            document.body.appendChild(target);

            triggerHandlers('swapHtml', {
                target: '#target',
                html: '',
                strategy: 'delete',
            });

            const exists = document.getElementById('target') !== null;
            expect(exists).toBe(false);
        });

        it('processes new elements after swap', async () => {
            const { triggerHandlers, onAttr, clearProcessedElements } =
                await import('../../src/core');
            await import('../../src/idiomorph');

            clearProcessedElements();

            const processedAttrs: string[] = [];

            onAttr('v-test-attr', ({ attr }) => {
                processedAttrs.push(attr);
            });

            const target = document.createElement('div');
            target.id = 'target';
            document.body.appendChild(target);

            triggerHandlers('swapHtml', {
                target: '#target',
                html: '<div id="target"><span v-test-attr="test-value">New</span></div>',
                strategy: 'this',
            });

            expect(processedAttrs).toContain('test-value');
        });
    });
});
