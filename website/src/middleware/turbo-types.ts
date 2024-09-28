export {};

declare global {
    interface Window {
        Turbo: {
            navigator: {
                history: {
                    replace(url: URL): void;
                    getRestorationDataForIdentifier(id: string): {
                        scrollPosition: { x: number; y: number };
                    };
                };
            };
        };
    }
}

// Document

export type TurboClickEvent = {
    detail?: {
        url: string;
        originalEvent: MouseEvent;
    };
} & Event;

export type TurboBeforeVisitEvent = {
    detail?: {
        url: string;
    };
} & Event;

export type TurboVisitEvent = {
    detail?: {
        url: string;
        action: 'advance' | 'replace' | 'restore';
    };
} & Event;

export type TurboBeforeCacheEvent = { detail?: unknown } & Event;

export type TurboBeforeRenderEvent = {
    detail?: {
        renderMethod: 'replace' | 'morph';
        newBody: HTMLBodyElement;
        isPreview: boolean;
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        resume: (value?: any) => void;
        render: (
            currentBody: HTMLBodyElement,
            newBody: HTMLBodyElement,
        ) => void;
    };
} & Event;

export type TurboRenderEvent = {
    detail?: {
        renderMethod: 'replace' | 'morph';
        isPreview: boolean;
    };
} & Event;

export type TurboLoadEvent = {
    detail?: {
        url: string;
        timing: {
            requestStart: number;
            requestEnd: number;
            visitStart: number;
            visitEnd: number;
        };
    };
} & Event;

// Page Refreshes

export type TurboMorphEvent = {
    detail?: {
        currentElement: Element;
        newElement: Element;
    };
} & Event;

export type TurboBeforeMorphElementEvent = {
    detail?: {
        newElement: Element;
    };
} & Event;

export type TurboBeforeMorphAttributeEvent = {
    detail?: {
        attributeName: string;
        mutationType: 'updated' | 'removed';
    };
} & Event;

export type TurboMorphElementEvent = {
    detail?: {
        newElement: Element;
    };
} & Event;

// Forms

export type TurboSubmitStartEvent = {
    detail?: {
        formSubmission: unknown;
    };
} & Event;

export type TurboSubmitEndEvent = {
    detail?: {
        success: boolean;
        fetchResponse: unknown | null;
        error: Error | null;
    };
} & Event;

// Frames

export type TurboBeforeFrameRenderEvent = {
    detail?: {
        newFrame: unknown;
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        resume: (value?: any) => void;
        render: (currentFrame: unknown, newFrame: unknown) => void;
    };
} & Event;

export type TurboFrameRenderEvent = {
    detail?: {
        fetchResponse: unknown;
    };
} & Event;

export type TurboFrameLoadEvent = { detail?: unknown } & Event;

export type TurboFrameMissingEvent = {
    detail?: {
        response: Response;
        visit: (location: string | URL, visitOptions: unknown) => Promise<void>;
    };
} & Event;

// Streams

export type TurboBeforeStreamRenderEvent = {
    detail?: {
        newStream: unknown;
        render: (currentElement: unknown) => Promise<void>;
    };
} & Event;

// HTTP Requests

export type TurboBeforeFetchRequestEvent = {
    detail?: {
        fetchOptions: RequestInit;
        url: URL;
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        resume: (value?: any) => void;
    };
} & Event;

export type TurboBeforeFetchResponseEvent = {
    detail?: {
        fetchResponse: unknown;
    };
} & Event;

export type TurboBeforePrefetchEvent = { detail?: unknown } & Event;

export type TurboFetchRequestErrorEvent = {
    detail?: {
        request: unknown;
        error: Error;
    };
} & Event;

export type TurboEvent =
    | TurboClickEvent
    | TurboBeforeVisitEvent
    | TurboVisitEvent
    | TurboBeforeCacheEvent
    | TurboRenderEvent
    | TurboLoadEvent
    | TurboMorphEvent
    | TurboBeforeMorphElementEvent
    | TurboBeforeMorphAttributeEvent
    | TurboMorphElementEvent
    | TurboSubmitStartEvent
    | TurboSubmitEndEvent
    | TurboBeforeFrameRenderEvent
    | TurboFrameRenderEvent
    | TurboFrameLoadEvent
    | TurboFrameMissingEvent
    | TurboBeforeStreamRenderEvent
    | TurboBeforeFetchRequestEvent
    | TurboBeforeFetchResponseEvent
    | TurboBeforePrefetchEvent
    | TurboFetchRequestErrorEvent;
