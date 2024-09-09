import { clusterProvider } from './cluster';

export const certManager = kubernetes.yaml.parse(
    { files: '../kubernetes/cert-manager.yaml'.substring(1) },
    { provider: clusterProvider, retainOnDelete: true },
)[0];
