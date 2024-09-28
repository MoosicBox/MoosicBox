import { createStore, produce } from 'solid-js/store';
import { ScanOrigin } from './api';
import { createListener, deepEqual } from './util';
import { onStartup, showScanStatusBanner } from './app';

export type ScanEventType =
    | FinishedScanEvent['type']
    | CountScanEvent['type']
    | ScannedScanEvent['type'];

export type FinishedScanEvent = {
    task: ScanTask;
    scanned: number;
    total: number;
    type: 'FINISHED';
};

export type CountScanEvent = {
    task: ScanTask;
    scanned: number;
    total: number;
    type: 'COUNT';
};

export type ScannedScanEvent = {
    task: ScanTask;
    scanned: number;
    total: number;
    type: 'SCANNED';
};

export type ScanEvent = (
    | FinishedScanEvent
    | CountScanEvent
    | ScannedScanEvent
) & { type: ScanEventType };

export type ScanTaskType = LocalScanTask['type'] | ApiScanTask['type'];

export type ScanTask = (LocalScanTask | ApiScanTask) & { type: ScanTaskType };

export type LocalScanTask = {
    paths: string[];
    type: 'LOCAL';
};

export type ApiScanTask = {
    paths: ScanOrigin;
    type: 'API';
};

export const onScanEventListener =
    createListener<(value: ScanEvent) => boolean | void>();
export const onScanEvent = onScanEventListener.on;
export const offScanEvent = onScanEventListener.off;

interface ScansState {
    tasks: {
        task: ScanTask;
        scanned: number;
        total: number;
    }[];
    hiddenTasks: ScanTask[];
}

export const [scanState, setScansState] = createStore<ScansState>({
    tasks: [],
    hiddenTasks: [],
});

export function hideTask(task: ScanTask) {
    setScansState(
        produce((state) => {
            state.hiddenTasks = [...state.hiddenTasks, task];
        }),
    );
}

function handleScanEvent(event: ScanEvent) {
    const eventType = event.type;

    switch (eventType) {
        case 'FINISHED':
            setScansState(
                produce((state) => {
                    const task = state.tasks.find((task) =>
                        deepEqual(task.task, event.task),
                    );
                    if (task) {
                        task.scanned = event.scanned;
                        task.total = event.total;
                    }
                }),
            );
            setTimeout(() => {
                setScansState(
                    produce((state) => {
                        state.tasks = state.tasks.filter(
                            (task) => !deepEqual(task.task, event.task),
                        );
                    }),
                );

                if (scanState.tasks.length === 0) {
                    showScanStatusBanner.set(false);
                }
            }, 5000);
            break;
        case 'COUNT':
            setScansState(
                produce((state) => {
                    const task = state.tasks.find((task) =>
                        deepEqual(task.task, event.task),
                    );
                    if (task) {
                        task.scanned = event.scanned;
                        task.total = event.total;
                    } else {
                        state.tasks.push({
                            task: event.task,
                            scanned: event.scanned,
                            total: event.total,
                        });
                    }
                }),
            );
            showScanStatusBanner.set(true);
            break;
        case 'SCANNED':
            setScansState(
                produce((state) => {
                    const task = state.tasks.find((task) =>
                        deepEqual(task.task, event.task),
                    );
                    if (task) {
                        task.scanned = event.scanned;
                        task.total = event.total;
                    } else {
                        state.tasks.push({
                            task: event.task,
                            scanned: event.scanned,
                            total: event.total,
                        });
                    }
                }),
            );
            showScanStatusBanner.set(true);
            break;
        default:
            eventType satisfies never;
            throw new Error(`Invalid ScanEvent type: '${eventType}'`);
    }
}

onScanEvent(handleScanEvent);

onStartup(async () => {
    setScansState(
        produce((state) => {
            state.tasks = [];
        }),
    );
});
