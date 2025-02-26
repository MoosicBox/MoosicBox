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

export function evaluate<T>(
    script: string,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    c: Record<string, unknown> & {
        element: HTMLElement;
        event?: Event | undefined;
    },
): T {
    function a(elements: (HTMLElement | null)[], f: (x: HTMLElement) => void) {
        elements.filter((x) => !!x).forEach(f);
    }
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    function s<
        T extends Exclude<keyof CSSStyleDeclaration, 'length' | 'parentRule'>,
    >(
        elements: (HTMLElement | null)[],
        name: T,
        value: CSSStyleDeclaration[T],
    ) {
        a(elements, (x) => {
            const dataName = `${(name as string)[0].toUpperCase()}${(name as string).slice(1)}`;
            x.dataset[`vReset${dataName}`] = x.style[name] as string;
            x.style[name] = value;
        });
    }
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    function r<
        T extends Exclude<keyof CSSStyleDeclaration, 'length' | 'parentRule'>,
    >(elements: (HTMLElement | null)[], name: T) {
        a(elements, (x) => {
            const dataName = `${(name as string)[0].toUpperCase()}${(name as string).slice(1)}`;
            x.style[name] = x.dataset[
                `vReset${dataName}`
            ] as CSSStyleDeclaration[T];
        });
    }
    return eval(script);
}

declare global {
    interface Window {
        triggerAction: typeof triggerAction;
    }
}

window['triggerAction'] = triggerAction;
