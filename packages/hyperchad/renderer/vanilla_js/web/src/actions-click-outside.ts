import { onAttr } from './core';

// eslint-disable-next-line @typescript-eslint/no-unused-vars
onAttr('v-onclickoutside', ({ element, attr }) => {
    // element.onclickoutside = () => {
    //     try {
    //         eval(attr);
    //     } catch (e) {
    //         console.error('onclickoutside failed', e);
    //     }
    // };
});
