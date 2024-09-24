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

    const containers = specJson.spec.template.spec.containers;

    containers.forEach(
        (container: {
            env: Record<string, string>[] | undefined;
            image: string | awsx.ecr.Image['imageUri'];
        }) => {
            container.image = image.imageUri;

            const env = container.env ?? [];

            env.push({
                name: 'CLUSTERS',
                value: `${domain}:moosicbox-tunnel-service-${$app.stage}:8004`,
            });

            container.env = env;
        },
    );

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
        ` moosicbox-tunnel-server-ingress-controller-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-ingress',
        ` moosicbox-tunnel-server-ingress-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(' tunnel.moosicbox.com', ` ${domain}`);

    return kubernetes.yaml.parse(
        { yaml: specYaml },
        { provider: clusterProvider, dependsOn },
    )[0];
}

export const repo = createEcrRepo();
export const image = createImage(repo);
export const certificate = createCertificate([]);
export const ingress = createIngress([certificate, image]);
export const issuer = createIssuer([ingress]);
export const loadBalancer = createLb(image, [issuer]);
export const nodePort = createNodePort([loadBalancer]);

export const outputs = {};
