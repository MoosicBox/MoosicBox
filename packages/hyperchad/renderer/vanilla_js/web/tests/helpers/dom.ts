/**
 * DOM test utilities for browser integration tests
 */

/**
 * Waits for an element to appear in the DOM
 */
export async function waitForElement(
    selector: string,
    timeout = 5000,
): Promise<Element> {
    const start = Date.now();
    while (Date.now() - start < timeout) {
        const element = document.querySelector(selector);
        if (element) {
            return element;
        }
        await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error(`Element ${selector} not found within ${timeout}ms`);
}

/**
 * Creates a temporary container for test HTML
 */
export function createTestContainer(): HTMLDivElement {
    const container = document.createElement('div');
    container.id = 'test-container';
    document.body.appendChild(container);
    return container;
}

/**
 * Cleans up the test container
 */
export function cleanupTestContainer(): void {
    const container = document.getElementById('test-container');
    if (container) {
        container.remove();
    }
}

/**
 * Sets innerHTML of test container and returns it
 */
export function setTestHTML(html: string): HTMLDivElement {
    cleanupTestContainer();
    const container = createTestContainer();
    container.innerHTML = html;
    return container;
}

/**
 * Simulates a click event on an element
 */
export function simulateClick(element: Element): void {
    const event = new MouseEvent('click', {
        bubbles: true,
        cancelable: true,
        view: window,
    });
    element.dispatchEvent(event);
}

/**
 * Simulates a keyboard event
 */
export function simulateKeyEvent(
    element: Element,
    type: 'keydown' | 'keyup',
    key: string,
    options: Partial<KeyboardEventInit> = {},
): void {
    const event = new KeyboardEvent(type, {
        key,
        bubbles: true,
        cancelable: true,
        ...options,
    });
    element.dispatchEvent(event);
}

/**
 * Simulates an input change event
 */
export function simulateChange(
    element: HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement,
    value: string,
): void {
    element.value = value;
    const event = new Event('change', { bubbles: true, cancelable: true });
    element.dispatchEvent(event);
}

/**
 * Simulates a mouse event
 */
export function simulateMouseEvent(
    element: Element,
    type: 'mousedown' | 'mouseup' | 'mouseover' | 'mouseout' | 'mousemove',
): void {
    const event = new MouseEvent(type, {
        bubbles: true,
        cancelable: true,
        view: window,
    });
    element.dispatchEvent(event);
}

/**
 * Waits for a custom event to be dispatched
 */
export function waitForEvent<T = unknown>(
    eventName: string,
    target: EventTarget = window,
    timeout = 5000,
): Promise<CustomEvent<T>> {
    return new Promise((resolve, reject) => {
        const timeoutId = setTimeout(() => {
            reject(
                new Error(
                    `Event ${eventName} not received within ${timeout}ms`,
                ),
            );
        }, timeout);

        target.addEventListener(
            eventName,
            (event) => {
                clearTimeout(timeoutId);
                resolve(event as CustomEvent<T>);
            },
            { once: true },
        );
    });
}

/**
 * Waits for a condition to be true
 */
export async function waitFor(
    condition: () => boolean | Promise<boolean>,
    timeout = 5000,
): Promise<void> {
    const start = Date.now();
    while (Date.now() - start < timeout) {
        if (await condition()) {
            return;
        }
        await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error(`Condition not met within ${timeout}ms`);
}
