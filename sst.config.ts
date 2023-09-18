import { SSTConfig } from 'sst';
import { API } from './stacks/MusicBoxStack';

export default {
    config(_input) {
        return {
            name: 'moosicbox-server',
            region: 'us-east-1',
        };
    },
    async stacks(app) {
        await app.stack(API);
    },
} satisfies SSTConfig;
