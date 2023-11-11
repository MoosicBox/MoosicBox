import { SSTConfig } from 'sst';
import { API } from './stacks/TunnelServerStack';

export default {
    config(_input) {
        return {
            name: 'moosicbox-tunnel-server',
            region: 'us-east-1',
        };
    },
    async stacks(app) {
        await app.stack(API);
    },
} satisfies SSTConfig;
