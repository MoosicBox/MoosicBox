document.addEventListener('keydown', (event) => {
    dispatchEvent(
        new CustomEvent(`v-key-down`, {
            detail: event.key,
        }),
    );
});

export {};
