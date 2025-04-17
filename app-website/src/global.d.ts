export type StartupCallback = () => void | Promise<void>;

declare global {
    interface Window {
        startupCallbacks: StartupCallback[];
        startedUp: boolean;
    }

    // eslint-disable-next-line no-var
    var startupCallbacks: StartupCallback[];
    // eslint-disable-next-line no-var
    var startedUp: boolean;
}
