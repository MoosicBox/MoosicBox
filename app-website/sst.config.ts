/// <reference path="./.sst/platform/config.d.ts" />
export default $config({
    app(input) {
        return {
            name: 'moosicbox-app',
            removal: input?.stage === 'prod' ? 'retain' : 'remove',
            home: 'aws',
            providers: {
                aws: { region: 'us-east-1' },
                cloudflare: {},
            },
        };
    },
    async run() {
        const { readdirSync } = await import('fs');
        const outputs = {};
        for (const value of readdirSync('./infra/')) {
            const result = await import(`./infra/${value}`);
            if (result.outputs) {
                Object.assign(outputs, result.outputs);
            }
        }
        return outputs;
    },
});
