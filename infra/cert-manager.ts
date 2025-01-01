import { all } from '@pulumi/pulumi';
import { clusterProvider } from './cluster';

const yaml = kubernetes.yaml.parse(
    { files: '../kubernetes/cert-manager.yaml'.substring(1) },
    { provider: clusterProvider },
);

export const certManagers = yaml.apply((x) => {
    return all(Object.values(x));
});
