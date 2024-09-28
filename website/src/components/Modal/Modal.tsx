import './modal.css';
import { type JSXElement, Show } from 'solid-js';

export interface ModalProps<T> {
    class?: string | undefined;
    show: () => T;
    onClose: () => void | boolean;
    children: JSXElement | ((value: NonNullable<T>) => JSXElement);
}

export default function modalFunc<T>(props: ModalProps<T>) {
    return (
        <Show when={props.show()}>
            {(value) => (
                <div class="moosicbox-modal-container" onClick={props.onClose}>
                    <div
                        class={`moosicbox-modal${props.class ? ` ${props.class}` : ''}`}
                        onClick={(e) => e.stopPropagation()}
                    >
                        {typeof props.children === 'function'
                            ? props.children(value())
                            : props.children}
                    </div>
                </div>
            )}
        </Show>
    );
}
