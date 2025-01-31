/// <reference path="./.sst/platform/config.d.ts" />
import { readdirSync } from 'fs';
export default $config({
    app(input) {
        return {
            name: 'moosicbox-marketing-site',
            removal: input?.stage === 'prod' ? 'retain' : 'remove',
            home: 'aws',
            providers: {
                aws: { region: 'us-east-1' },
                cloudflare: {},
            },
        };
    },
    async run() {
        const outputs = {};

        if (process.env.SKIP_INFRA === '1') {
            return outputs;
        }

        for (const value of readdirSync('./infra/')) {
            const result = await import(`./infra/${value}`);
            if (result.outputs) {
                Object.assign(outputs, result.outputs);
            }
        }

        return outputs;
    },
});
