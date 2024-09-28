import { c as createComponent, r as renderTemplate, a as addAttribute, e as renderHead, f as renderSlot, b as createAstro } from './astro/server_BHkv7Nwt.mjs';
/* empty css                          */

const $$Astro = createAstro();
const $$Layout = createComponent(($$result, $$props, $$slots) => {
  const Astro2 = $$result.createAstro($$Astro, $$props, $$slots);
  Astro2.self = $$Layout;
  const { title } = Astro2.props;
  return renderTemplate`<html lang="en"> <head><meta charset="UTF-8"><meta name="description" content="Astro description"><meta name="viewport" content="width=device-width"><meta name="turbo-cache-control" content="no-cache"><link rel="icon" type="image/ico" href="/favicon.ico"><meta name="generator"${addAttribute(Astro2.generator, "content")}><title>${title}</title>${renderHead()}</head> <body> <div id="root" class="dark"> <section class="navigation-bar-and-main-content"> <main class="main-content"> ${renderSlot($$result, $$slots["default"])} </main> </section> </div> </body></html>`;
}, "/home/bsteffaniak/GitHub/MoosicBoxUI/src/pages/setup/Layout.astro", void 0);

const $$file = "/home/bsteffaniak/GitHub/MoosicBoxUI/src/pages/setup/Layout.astro";
const $$url = "/setup/Layout";

const _page = /*#__PURE__*/Object.freeze(/*#__PURE__*/Object.defineProperty({
    __proto__: null,
    default: $$Layout,
    file: $$file,
    url: $$url
}, Symbol.toStringTag, { value: 'Module' }));

export { $$Layout as $, _page as _ };
//# sourceMappingURL=Layout_BRzU7R80.mjs.map
