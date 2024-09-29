/**
 * Hydrate after the DOMContentLoaded event
 * See https://github.com/withastro/astro/issues/8178
 * @type {import('astro').ClientDirective}
 */
export default (load) => {
    const todo = async () => {
        const hydrate = await load();
        await hydrate();
    };
    // https://stackoverflow.com/questions/39993676/code-inside-domcontentloaded-event-not-working
    if (document.readyState != 'loading') todo();
    else window.addEventListener('DOMContentLoaded', todo);
};
