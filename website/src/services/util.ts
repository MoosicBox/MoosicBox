import { isServer } from 'solid-js/web';
import type { Entries } from './types';
import { createSignal, onCleanup, onMount } from 'solid-js';
import { atom } from 'nanostores';
import type { WritableAtom } from 'nanostores';
import { persistentAtom } from '@nanostores/persistent';

type ListenerMapping<T> = {
    baseListener: (value: T) => void;
    listener: (value: T, prevValue: T) => void;
};

export class ClientAtom<T> {
    private _atom: WritableAtom<T>;
    private _initial: T;
    private _prevValue: T;
    private _name: string | undefined;
    private _listeners: ListenerMapping<T>[] = [];

    constructor(initial: T, name?: string | undefined) {
        this._initial = initial;
        this._name = name;
        if (name) {
            this._atom = persistentAtom(name, initial, {
                encode: JSON.stringify,
                decode: JSON.parse,
            });
        } else {
            this._atom = atom(initial);
        }

        this._prevValue = this.get();
    }

    get name(): string | undefined {
        return this._name;
    }

    get initial(): T {
        return this._initial;
    }

    get(): T {
        return this._atom.get();
    }

    set(value: T) {
        this._atom.set(value);
        this._prevValue = value;
    }

    listen(listener: (value: T, prevValue: T) => void) {
        const mapping: ListenerMapping<T> = {
            baseListener: (v: T) => listener(v, this._prevValue),
            listener,
        };
        this._atom.listen(mapping.baseListener);
        this._listeners.push(mapping);
    }

    off(listener: (value: T, prevValue: T) => void) {
        const index = this._listeners.findIndex((x) => x.listener === listener);
        if (index !== -1) {
            this._listeners.splice(index, 1);
            this._atom.off();
            this._listeners.forEach((mapping) => {
                this._atom.listen(mapping.baseListener);
            });
        }
    }
}

export function clientAtom<T>(
    initial: T,
    name?: string | undefined,
): ClientAtom<T> {
    return new ClientAtom<T>(initial, name);
}

export function clientSignal<T>(
    atom: ClientAtom<T>,
): [() => T, (value: T) => void] {
    let init = true;

    const [get, set] = createSignal<T>(atom.get(), {
        equals(a, b) {
            if (init) {
                init = false;
                return false;
            }
            return a === b;
        },
    });

    const listener = (value: T) => {
        set(value as Parameters<typeof set>[0]);
    };

    onMount(() => {
        set(atom.get() as Parameters<typeof set>[0]);
        atom.listen(listener);
    });
    onCleanup(() => {
        atom.off(listener);
    });

    return [
        () => {
            const wasInit = init;
            const value = get();

            if (wasInit) {
                return atom.initial;
            } else {
                return value;
            }
        },
        (value: T) => {
            atom.set(value);
        },
    ];
}

type BaseCallbackType = (
    ...args: any // eslint-disable-line @typescript-eslint/no-explicit-any
) => boolean | void | Promise<boolean | void>;
export function createListener<CallbackType extends BaseCallbackType>(): {
    on: (callback: CallbackType) => CallbackType;
    onFirst: (callback: CallbackType) => CallbackType;
    off: (callback: CallbackType) => void;
    listeners: CallbackType[];
    trigger: CallbackType;
} {
    let listeners: CallbackType[] = [];
    function on(callback: CallbackType): CallbackType {
        listeners.push(callback);
        return callback;
    }
    function onFirst(callback: CallbackType): CallbackType {
        listeners.unshift(callback);
        return callback;
    }
    function off(callback: CallbackType): void {
        listeners = listeners.filter((c) => c !== callback);
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const trigger = (...args: any) => {
        for (const listener of listeners) {
            if (listener(...args) === false) {
                break;
            }
        }
    };

    return { on, onFirst, off, listeners, trigger: trigger as CallbackType };
}

export function orderedEntries<T extends Parameters<typeof Object.entries>[0]>(
    value: T,
    order: (keyof T)[],
): Entries<T> {
    const updates = Object.entries(value) as Entries<T>;

    updates.sort(([key1], [key2]) => {
        let first = order.indexOf(key1 as keyof T);
        let second = order.indexOf(key2 as keyof T);
        first = first === -1 ? order.length : first;
        second = second === -1 ? order.length : second;

        return first - second;
    });

    return updates;
}

export class QueryParams {
    private params: [string, string][];

    public constructor(
        init?: Record<string, string | undefined> | QueryParams | string,
    ) {
        this.params = [];

        if (typeof init === 'string') {
            if (init[0] === '?') {
                init = init.substring(1);
            }

            if (init.trim().length > 0) {
                init.split('&')
                    .map((pair) => pair.split('='))
                    .forEach(([key, value]) => {
                        this.params.push([key!, value!]);
                    });
            }
        } else if (init instanceof QueryParams) {
            this.params.push(...init.params);
        } else if (init) {
            Object.entries(init).forEach(([key, value]) => {
                if (typeof value === 'undefined') return;
                this.params.push([key, value]);
            });
        }
    }

    public get size(): number {
        return this.params.length;
    }

    public has(key: string): boolean {
        return !!this.params.find(([k, _value]) => k === key);
    }

    public get(key: string): string | undefined {
        const value = this.params.find(([k, _value]) => k === key);

        if (value) {
            return value[1];
        }

        return undefined;
    }

    public set(key: string, value: string) {
        const existing = this.params.find(([k, _value]) => k === key);

        if (existing) {
            existing[1] = value;
        } else {
            this.params.push([key, value]);
        }
    }

    public delete(key: string) {
        this.params = this.params.filter(([k, _value]) => k !== key);
    }

    public forEach(func: (key: string, value: string) => void) {
        this.params.forEach(([key, value]) => func(key, value));
    }

    public toString(): string {
        return `${this.params
            .map(
                ([key, value]) =>
                    `${encodeURIComponent(key)}=${encodeURIComponent(value)}`,
            )
            .join('&')}`;
    }
}

export function getQueryParam(key: string) {
    const url = new URL(window.location.href);

    return url.searchParams.get(key);
}

export function setQueryParam(key: string, value: string | undefined) {
    const url = new URL(window.location.href);

    if (typeof value === 'undefined') {
        url.searchParams.delete(key);
    } else {
        if (url.searchParams.get(key) === value) {
            console.debug('Query param', key, 'is already set');
            return;
        }
        url.searchParams.set(key, value);
    }

    console.debug('Replacing url state with', url.toString());

    window.history.replaceState({}, '', url);
    window.Turbo.navigator.history.replace(url);
}

export function historyBack() {
    window.history.back();
}

export function isMobile() {
    if (isServer) return false;

    return isUserAgentMobile(
        navigator.userAgent || (('opera' in window && window.opera) as string),
    );
}

export function isUserAgentMobile(userAgent: string | null | undefined) {
    if (!userAgent) return false;

    return (
        /(android|bb\d+|meego).+mobile|avantgo|bada\/|blackberry|blazer|compal|elaine|fennec|hiptop|iemobile|ip(hone|od)|iris|kindle|lge |maemo|midp|mmp|mobile.+firefox|netfront|opera m(ob|in)i|palm( os)?|phone|p(ixi|re)\/|plucker|pocket|psp|series(4|6)0|symbian|treo|up\.(browser|link)|vodafone|wap|windows ce|xda|xiino/i.test(
            userAgent,
        ) ||
        /1207|6310|6590|3gso|4thp|50[1-6]i|770s|802s|a wa|abac|ac(er|oo|s-)|ai(ko|rn)|al(av|ca|co)|amoi|an(ex|ny|yw)|aptu|ar(ch|go)|as(te|us)|attw|au(di|-m|r |s )|avan|be(ck|ll|nq)|bi(lb|rd)|bl(ac|az)|br(e|v)w|bumb|bw-(n|u)|c55\/|capi|ccwa|cdm-|cell|chtm|cldc|cmd-|co(mp|nd)|craw|da(it|ll|ng)|dbte|dc-s|devi|dica|dmob|do(c|p)o|ds(12|-d)|el(49|ai)|em(l2|ul)|er(ic|k0)|esl8|ez([4-7]0|os|wa|ze)|fetc|fly(-|_)|g1 u|g560|gene|gf-5|g-mo|go(\.w|od)|gr(ad|un)|haie|hcit|hd-(m|p|t)|hei-|hi(pt|ta)|hp( i|ip)|hs-c|ht(c(-| |_|a|g|p|s|t)|tp)|hu(aw|tc)|i-(20|go|ma)|i230|iac( |-|\/)|ibro|idea|ig01|ikom|im1k|inno|ipaq|iris|ja(t|v)a|jbro|jemu|jigs|kddi|keji|kgt( |\/)|klon|kpt |kwc-|kyo(c|k)|le(no|xi)|lg( g|\/(k|l|u)|50|54|-[a-w])|libw|lynx|m1-w|m3ga|m50\/|ma(te|ui|xo)|mc(01|21|ca)|m-cr|me(rc|ri)|mi(o8|oa|ts)|mmef|mo(01|02|bi|de|do|t(-| |o|v)|zz)|mt(50|p1|v )|mwbp|mywa|n10[0-2]|n20[2-3]|n30(0|2)|n50(0|2|5)|n7(0(0|1)|10)|ne((c|m)-|on|tf|wf|wg|wt)|nok(6|i)|nzph|o2im|op(ti|wv)|oran|owg1|p800|pan(a|d|t)|pdxg|pg(13|-([1-8]|c))|phil|pire|pl(ay|uc)|pn-2|po(ck|rt|se)|prox|psio|pt-g|qa-a|qc(07|12|21|32|60|-[2-7]|i-)|qtek|r380|r600|raks|rim9|ro(ve|zo)|s55\/|sa(ge|ma|mm|ms|ny|va)|sc(01|h-|oo|p-)|sdk\/|se(c(-|0|1)|47|mc|nd|ri)|sgh-|shar|sie(-|m)|sk-0|sl(45|id)|sm(al|ar|b3|it|t5)|so(ft|ny)|sp(01|h-|v-|v )|sy(01|mb)|t2(18|50)|t6(00|10|18)|ta(gt|lk)|tcl-|tdg-|tel(i|m)|tim-|t-mo|to(pl|sh)|ts(70|m-|m3|m5)|tx-9|up(\.b|g1|si)|utst|v400|v750|veri|vi(rg|te)|vk(40|5[0-3]|-v)|vm40|voda|vulc|vx(52|53|60|61|70|80|81|83|85|98)|w3c(-| )|webc|whit|wi(g |nc|nw)|wmlb|wonu|x700|yas-|your|zeto|zte-/i.test(
            userAgent.substring(0, 4),
        )
    );
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function areEqualShallow(a: any, b: any) {
    for (const key in a) {
        if (!(key in b) || a[key] !== b[key]) {
            return false;
        }
    }
    for (const key in b) {
        if (!(key in a) || a[key] !== b[key]) {
            return false;
        }
    }
    return true;
}

function circularStringify(obj: object): string {
    const getCircularReplacer = () => {
        const seen = new WeakSet();
        return (_key: string, value: unknown) => {
            if (typeof value === 'object' && value !== null) {
                if (seen.has(value)) {
                    return '[[circular]]';
                }
                seen.add(value);
            }
            return value;
        };
    };

    return JSON.stringify(obj, getCircularReplacer());
}

export function objToStr(obj: unknown): string {
    if (typeof obj === 'string') {
        return obj;
    } else if (typeof obj === 'undefined') {
        return 'undefined';
    } else if (obj === null) {
        return 'null';
    } else if (typeof obj === 'object') {
        return circularStringify(obj);
    } else {
        return obj.toString();
    }
}

export const trace = console.trace;

export function throwExpression(errorMessage: string): never {
    throw new Error(errorMessage);
}

export function deepEqual<T>(x: T, y: T) {
    if (x === y) {
        return true;
    } else if (
        typeof x == 'object' &&
        x != null &&
        typeof y == 'object' &&
        y != null
    ) {
        if (Object.keys(x).length != Object.keys(y).length) return false;

        for (const prop in x) {
            // eslint-disable-next-line no-prototype-builtins
            if (y.hasOwnProperty(prop)) {
                if (!deepEqual(x[prop], y[prop])) return false;
            } else return false;
        }

        return true;
    } else return false;
}

export function onlyUnique<T>(value: T, index: number, array: T[]): boolean {
    return array.indexOf(value) === index;
}
