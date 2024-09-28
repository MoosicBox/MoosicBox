import './tabs.css';
import {
    createComputed,
    createSignal,
    For,
    type JSXElement,
    Show,
} from 'solid-js';

export interface TabsProps {
    class?: string | undefined;
    tabs: { [key: string]: string };
    default: string;
    children: (value: NonNullable<string>) => JSXElement;
}

export default function tabsFunc(props: TabsProps) {
    const [selected, setSelected] = createSignal(props.default);
    const [tabs, setTabs] = createSignal<{ key: string; display: string }[]>(
        [],
    );

    createComputed(() => {
        setSelected(props.default);
        setTabs(
            Object.entries(props.tabs).map(([key, display]) => {
                return { key, display };
            }),
        );
    });

    return (
        <div class="moosicbox-tabs">
            <div class="moosicbox-tabs-headers">
                <For each={tabs()}>
                    {(header) => (
                        <button
                            type="button"
                            class="moosicbox-tabs-headers-header"
                            onClick={() => setSelected(header.key)}
                        >
                            {header.display}
                        </button>
                    )}
                </For>
            </div>
            <Show when={selected()}>
                {(value) => (
                    <div class="moosicbox-tabs-content">
                        {props.children(value())}
                    </div>
                )}
            </Show>
        </div>
    );
}
