import fs from 'node:fs';
import { parse } from 'yaml';
import { clusterProvider } from './cluster';
import { Input, Resource } from '@pulumi/pulumi';

export const domainName = 'moosicbox.com';
export const domain =
    $app.stage === 'prod'
        ? `tunnel.${domainName}`
        : `tunnel-${$app.stage}.${domainName}`;

function createEcrRepo() {
    return new awsx.ecr.Repository('moosicbox-load-balancer', {
        forceDelete: true,
    });
}

function createImage(repo: awsx.ecr.Repository) {
    const context = `../..`;
    return new awsx.ecr.Image('load-balancer', {
        repositoryUrl: repo.url,
        context,
        dockerfile: `${context}/packages/load_balancer/LoadBalancer.Dockerfile`,
    });
}

function createCertificate(dependsOn: Input<Input<Resource>[]>) {
    let specYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-certificate.yaml'.substring(
            1,
        ),
        'utf8',
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-cert',
        ` moosicbox-tunnel-server-cert-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-issuer',
        ` moosicbox-tunnel-server-issuer-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(' tunnel.moosicbox.com', ` ${domain}`);

    return kubernetes.yaml.parse(
        { yaml: specYaml },
        { provider: clusterProvider, dependsOn },
    )[0];
}

function createIngress(dependsOn: Input<Input<Resource>[]>) {
    let specYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-ingress.yaml'.substring(
            1,
        ),
        'utf8',
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-ingress',
        ` moosicbox-tunnel-server-ingress-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-issuer',
        ` moosicbox-tunnel-server-issuer-${$app.stage}`,
    );
    const specJson = parse(specYaml);

    delete specJson.metadata.annotations['cert-manager.io/issuer'];

    return new kubernetes.networking.v1.Ingress('tunnel-server', specJson, {
        provider: clusterProvider,
        dependsOn,
    });
}

function createIssuer(dependsOn: Input<Input<Resource>[]>) {
    let specYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-issuer.yaml'.substring(
            1,
        ),
        'utf8',
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-ingress',
        ` moosicbox-tunnel-server-ingress-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-issuer',
        ` moosicbox-tunnel-server-issuer-${$app.stage}`,
    );

    return kubernetes.yaml.parse(
        { yaml: specYaml },
        { provider: clusterProvider, dependsOn },
    )[0];
}

function createLb(image: awsx.ecr.Image, dependsOn: Input<Input<Resource>[]>) {
    let specYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-lb-deployment.yaml'.substring(
            1,
        ),
        'utf8',
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-lb',
        ` moosicbox-tunnel-server-lb-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-cert',
        ` moosicbox-tunnel-server-cert-${$app.stage}`,
    );
    const specJson = parse(specYaml);

    type Container = {
        ports: { hostPort: number }[];
        volumeMounts: unknown[] | undefined;
        env: Record<string, string>[] | undefined;
        image: string | awsx.ecr.Image['imageUri'];
    };

    specJson.spec.template.spec.volumes = [];

    const containers: Container[] = specJson.spec.template.spec.containers;

    containers.forEach((container) => {
        container.image = image.imageUri;
        container.volumeMounts = [];

        const env = container.env ?? [];

        env.push({
            name: 'CLUSTERS',
            value: `${domain}:moosicbox-tunnel-service-${$app.stage}:8004`,
        });

        container.env = env;

        container.ports = container.ports.filter((x) => x.hostPort === 80);
    });

    return new kubernetes.apps.v1.Deployment('tunnel-server-lb', specJson, {
        provider: clusterProvider,
        dependsOn,
    });
}

function createNodePort(dependsOn: Input<Input<Resource>[]>) {
    let specYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-nodeport.yaml'.substring(
            1,
        ),
        'utf8',
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-ingress-controller',
        ` moosicbox-tunnel-server-temp-ingress-controller-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-ingress',
        ` moosicbox-tunnel-server-ingress-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-temp-',
        ` moosicbox-tunnel-server-`,
    );
    specYaml = specYaml.replaceAll(' tunnel.moosicbox.com', ` ${domain}`);

    return kubernetes.yaml.parse(
        { yaml: specYaml },
        { provider: clusterProvider, dependsOn },
    )[0];
}

export const repo = createEcrRepo();
export const image = createImage(repo);
// export const certificate = createCertificate([]);
// export const issuer = createIssuer([]);
export const ingress = createIngress([]);
export const loadBalancer = createLb(image, []);
export const nodePort = createNodePort([]);

export const outputs = {};
