import { clusterProvider } from './cluster';

export const certManagers = kubernetes.yaml.parse(
    { files: '../kubernetes/cert-manager.yaml'.substring(1) },
    { provider: clusterProvider },
)[
    'apiextensions.k8s.io/v1/CustomResourceDefinition::certificaterequests.cert-manager.io'
];
