document.addEventListener('keyup', (event) => {
    dispatchEvent(
        new CustomEvent(`v-key-up`, {
            detail: event.key,
        }),
    );
});

export {};
