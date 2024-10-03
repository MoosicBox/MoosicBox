/// <reference path="./.sst/platform/config.d.ts" />

import { readdirSync } from 'fs';

export default $config({
    app(input) {
        return {
            name: 'moosicbox-app',
            removal: input?.stage === 'prod' ? 'retain' : 'remove',
            home: 'aws',
            providers: {
                aws: {
                    region: 'us-east-1',
                    aws: { region: 'us-east-1', version: '6.52.0' },
                },
                cloudflare: '5.40.0',
            },
        };
    },
    async run() {
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
