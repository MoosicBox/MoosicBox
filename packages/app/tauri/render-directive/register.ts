import type { AstroIntegration } from 'astro';

export default function (): AstroIntegration {
    return {
        name: 'client:render',
        hooks: {
            'astro:config:setup': ({ addClientDirective }) => {
                addClientDirective({
                    name: 'click',
                    entrypoint: './render-directive/render.js',
                });
            },
        },
    };
}
