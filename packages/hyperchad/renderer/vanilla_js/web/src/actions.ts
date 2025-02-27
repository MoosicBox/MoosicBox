export function triggerAction(action: {
    action: unknown;
    value?: unknown | undefined;
}) {
    fetch('$action', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(action),
    });
}

const throttles: { [id: string]: { last: number } } = {};

export function evaluate<T>(
    script: string,
    c: Record<string, unknown> & {
        element: HTMLElement;
        event?: Event | undefined;
    },
): T {
    // for all non-null elements
    function a(elements: (HTMLElement | null)[], f: (x: HTMLElement) => void) {
        elements.filter((x) => !!x).forEach(f);
    }

    // data_reset_name
    function drn(name: string): string {
        return `vReset${name[0].toUpperCase()}${name.slice(1)}`;
    }

    // set_style
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    function ss<
        T extends Exclude<keyof CSSStyleDeclaration, 'length' | 'parentRule'>,
    >(
        elements: (HTMLElement | null)[],
        name: T,
        value: CSSStyleDeclaration[T],
    ) {
        a(elements, (x) => {
            const dataName = drn(name as string);
            if (typeof x.dataset[dataName] === 'undefined') {
                x.dataset[dataName] = x.style[name] as string;
            }
            x.style[name] = value;
        });
    }
    // reset_style
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    function rs<
        T extends Exclude<keyof CSSStyleDeclaration, 'length' | 'parentRule'>,
    >(elements: (HTMLElement | null)[], name: T) {
        a(elements, (x) => {
            x.style[name] = x.dataset[
                drn(name as string)
            ] as CSSStyleDeclaration[T];
        });
    }

    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    function throttle(f: () => T, duration: number): T | undefined {
        if (!c.element.id) return;

        const existing = throttles[c.element.id];
        const now = Date.now();

        if (!existing) {
            const t = { last: now };
            throttles[c.element.id] = t;
            return f();
        }

        if (now - existing.last >= duration) {
            existing.last = now;
            return f();
        }
    }

    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const delay = setTimeout;

    return eval(script);
}

declare global {
    interface Window {
        triggerAction: typeof triggerAction;
    }
}

window['triggerAction'] = triggerAction;
