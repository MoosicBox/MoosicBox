function newCluster() {
    const example = digitalocean.getKubernetesVersions({});

    return new digitalocean.KubernetesCluster(
        'MoosicBox',
        {
            name: 'moosicbox-prod',
            ha: false,
            version: example.then((example) => example.latestVersion),
            region: digitalocean.Region.NYC1,
            nodePool: {
                name: 'moosicbox-prod-pool',
                autoScale: false,
                size: digitalocean.DropletSlug.DropletS1VCPU2GB,
                nodeCount: 1,
                minNodes: 1,
                maxNodes: 2,
            },
        },
        {
            retainOnDelete: true,
        },
    );
}

let existing:
    | Awaited<ReturnType<typeof digitalocean.getKubernetesCluster>>
    | undefined;

try {
    existing = await digitalocean.getKubernetesCluster({
        name: 'moosicbox-prod',
    });
} catch {
    console.log('No existing cluster');
}

export const cluster = existing ?? newCluster();

export const kubeconfig = cluster.kubeConfigs[0].rawConfig;

export const clusterProvider = new kubernetes.Provider('do-k8s', {
    kubeconfig,
});
