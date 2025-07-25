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
const styleOverrides: { [id: string]: Set<string> } = {};

export function evaluate<T>(
    script: string,
    c: Record<string, unknown> & {
        element: HTMLElement;
        event?: Event | undefined;
        value?: unknown;
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

    // call_function
    function cf(
        elements: (HTMLElement | null)[],
        name: keyof HTMLElement,
        ...args: unknown[]
    ) {
        a(elements, (x) => {
            (x[name] as (...x: unknown[]) => void).apply(x, args);
        });
    }

    // set_style
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

            if (x.id && c.element.id) {
                (styleOverrides[x.id] = styleOverrides[x.id] ?? new Set()).add(
                    c.element.id,
                );
            }
        });
    }

    // reset_style
    function rs<
        T extends Exclude<keyof CSSStyleDeclaration, 'length' | 'parentRule'>,
    >(elements: (HTMLElement | null)[], name: T) {
        a(elements, (x) => {
            if (x.id && c.element.id) {
                const o = styleOverrides[x.id];
                o?.delete(c.element.id);
                if ((o?.size ?? 0) > 0) return;
            }

            x.style[name] = x.dataset[
                drn(name as string)
            ] as CSSStyleDeclaration[T];
        });
    }

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

    function delay(f: () => void, ms: number) {
        setTimeout(f, ms);
    }

    // prettier-ignore
    // get_context
    const g = () => ({
        'a': a,
        'drn': drn,
        'ss': ss,
        'cf': cf,
        'rs': rs,
        'throttle': throttle,
        'delay': delay,
        'element': c.element,
        'event': c.event,
        'value': c.value,
    });

    return eval(`const ctx=${g.name}();${script}`);
}

declare global {
    interface Window {
        triggerAction: typeof triggerAction;
    }
}

window['triggerAction'] = triggerAction;

const setupListeners = new Set<string>();

export function createEventDelegator(
    eventType: string,
    attrName: string,
    handler: (element: HTMLElement, attr: string, event: Event) => void,
) {
    if (!setupListeners.has(eventType)) {
        document.addEventListener(
            eventType,
            (event: Event) => {
                const target = event.target as HTMLElement;
                if (!target) return;

                // Find the element with the attribute by walking up the DOM tree
                let currentElement = target;
                while (currentElement && currentElement !== document.body) {
                    const attr = currentElement.getAttribute(attrName);
                    if (attr) {
                        handler(currentElement, attr, event);
                        return;
                    }
                    currentElement =
                        currentElement.parentElement as HTMLElement;
                }
            },
            true,
        );
        setupListeners.add(eventType);
    }
}
