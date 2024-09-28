import { c as createComponent, r as renderTemplate, d as renderComponent, m as maybeRenderHead } from '../../chunks/astro/server_BHkv7Nwt.mjs';
import { $ as $$Layout } from '../../chunks/Layout_BRzU7R80.mjs';
export { renderers } from '../../renderers.mjs';

const $$Hello = createComponent(($$result, $$props, $$slots) => {
  return renderTemplate`${renderComponent($$result, "Layout", $$Layout, { "title": "Hello | MoosicBox" }, { "default": ($$result2) => renderTemplate` ${maybeRenderHead()}<p>hello!</p> <a href="./music">Next</a> ` })}`;
}, "/home/bsteffaniak/GitHub/MoosicBoxUI/src/pages/setup/hello.astro", void 0);

const $$file = "/home/bsteffaniak/GitHub/MoosicBoxUI/src/pages/setup/hello.astro";
const $$url = "/setup/hello";

const _page = /*#__PURE__*/Object.freeze(/*#__PURE__*/Object.defineProperty({
    __proto__: null,
    default: $$Hello,
    file: $$file,
    url: $$url
}, Symbol.toStringTag, { value: 'Module' }));

const page = () => _page;

export { page };
//# sourceMappingURL=hello.astro.mjs.map
