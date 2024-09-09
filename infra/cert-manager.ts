import { clusterProvider } from './cluster';

export const certManagers = kubernetes.yaml.parse(
    { files: '../kubernetes/cert-manager.yaml'.substring(1) },
    { provider: clusterProvider, retainOnDelete: true },
)[
    'apiextensions.k8s.io/v1/CustomResourceDefinition::certificaterequests.cert-manager.io'
];
// .apply((x) => {
//     const value = Object.entries(x);
//     console.log(value);
//     return value;
// });
