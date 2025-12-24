import { describe, it, expect, beforeEach } from 'vitest';

describe('actions-change', () => {
    beforeEach(() => {
        document.body.innerHTML = '';
        // Clean up window test globals
        Object.keys(window)
            .filter((k) => k.startsWith('__') && !k.startsWith('__vitest'))
            .forEach(
                (k) => delete (window as unknown as Record<string, unknown>)[k],
            );
    });

    it('triggers on input change with v-onchange attribute', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-change');

        const input = document.createElement('input');
        input.id = 'input';
        input.type = 'text';
        input.setAttribute('v-onchange', 'window.__changed = true');
        document.body.appendChild(input);

        input.value = 'new value';
        input.dispatchEvent(new Event('change', { bubbles: true }));

        const changed = (window as unknown as Record<string, boolean>)
            .__changed;
        expect(changed).toBe(true);
    });

    it('triggers on select change', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-change');

        const select = document.createElement('select');
        select.id = 'select';
        select.setAttribute('v-onchange', 'window.__selectedValue = ctx.value');

        const option1 = document.createElement('option');
        option1.value = 'opt1';
        option1.textContent = 'Option 1';

        const option2 = document.createElement('option');
        option2.value = 'opt2';
        option2.textContent = 'Option 2';

        select.appendChild(option1);
        select.appendChild(option2);
        document.body.appendChild(select);

        select.value = 'opt2';
        select.dispatchEvent(new Event('change', { bubbles: true }));

        const selectedValue = (window as unknown as Record<string, string>)
            .__selectedValue;
        expect(selectedValue).toBe('opt2');
    });

    it('triggers on input event as well', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-change');

        const input = document.createElement('input');
        input.id = 'input';
        input.type = 'text';
        input.setAttribute('v-onchange', 'window.__inputValue = ctx.value');
        document.body.appendChild(input);

        input.value = 'typed value';
        input.dispatchEvent(new Event('input', { bubbles: true }));

        const inputValue = (window as unknown as Record<string, string>)
            .__inputValue;
        expect(inputValue).toBe('typed value');
    });

    it('provides element in context', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-change');

        const input = document.createElement('input');
        input.id = 'my-input';
        input.type = 'text';
        input.setAttribute('v-onchange', 'window.__elementId = ctx.element.id');
        document.body.appendChild(input);

        input.value = 'test';
        input.dispatchEvent(new Event('change', { bubbles: true }));

        const elementId = (window as unknown as Record<string, string>)
            .__elementId;
        expect(elementId).toBe('my-input');
    });

    it('provides value in context from input', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-change');

        const input = document.createElement('input');
        input.id = 'input';
        input.type = 'text';
        input.setAttribute('v-onchange', 'window.__value = ctx.value');
        document.body.appendChild(input);

        input.value = 'test-value-123';
        input.dispatchEvent(new Event('change', { bubbles: true }));

        const value = (window as unknown as Record<string, string>).__value;
        expect(value).toBe('test-value-123');
    });

    it('handles checkbox checked state', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-change');

        const checkbox = document.createElement('input');
        checkbox.id = 'checkbox';
        checkbox.type = 'checkbox';
        checkbox.setAttribute(
            'v-onchange',
            'window.__checked = ctx.element.checked',
        );
        document.body.appendChild(checkbox);

        checkbox.checked = true;
        checkbox.dispatchEvent(new Event('change', { bubbles: true }));

        const checked = (window as unknown as Record<string, boolean>)
            .__checked;
        expect(checked).toBe(true);
    });

    it('bubbles up DOM tree to find v-onchange attribute', async () => {
        await import('../../src/core');
        await import('../../src/actions');
        await import('../../src/actions-change');

        const parent = document.createElement('div');
        parent.id = 'parent';
        parent.setAttribute('v-onchange', 'window.__parentTriggered = true');

        const input = document.createElement('input');
        input.id = 'child-input';
        input.type = 'text';

        parent.appendChild(input);
        document.body.appendChild(parent);

        input.value = 'test';
        input.dispatchEvent(new Event('change', { bubbles: true }));

        const parentTriggered = (window as unknown as Record<string, boolean>)
            .__parentTriggered;
        expect(parentTriggered).toBe(true);
    });
});
