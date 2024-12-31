import fs from 'node:fs';
import { parse } from 'yaml';
import { clusterProvider } from './cluster';
import { Input, Output, Resource } from '@pulumi/pulumi';
import { registryAuth, repo } from './registry';

const imageName = $interpolate`${repo.endpoint}/tunnel-server`;

function createImage() {
    const context = `../..`;
    return new docker.Image('tunnel-server', {
        imageName,
        build: {
            builderVersion: 'BuilderBuildKit',
            args: {
                BUILDKIT_INLINE_CACHE: '1',
            },
            cacheFrom: {
                images: [$interpolate`${imageName}:cache-base`],
            },
            platform: 'linux/amd64',
            context: context,
            dockerfile: `${context}/packages/tunnel_server/TunnelServer.Dockerfile`,
        },
        registry: registryAuth,
    });
}

function createDeployment(
    image: docker.Image,
    dependsOn: Input<Input<Resource>[]>,
) {
    let specYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-deployment.yaml'.substring(
            1,
        ),
        'utf8',
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server',
        ` moosicbox-tunnel-server-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-service',
        ` moosicbox-tunnel-service-${$app.stage}`,
    );
    const specJson = parse(specYaml);

    type Container = {
        ports: { hostPort: number }[];
        volumeMounts: unknown[] | undefined;
        env: Record<string, string>[] | undefined;
        image: string | Output<string>;
    };

    const containers: Container[] = specJson.spec.template.spec.containers;

    containers.forEach((container) => {
        container.image = $interpolate`${image.imageName}`;

        if (
            process.env.AWS_ACCESS_KEY_ID &&
            process.env.AWS_SECRET_ACCESS_KEY
        ) {
            container.env = container.env ?? [];
            container.env.push({
                name: 'AWS_ACCESS_KEY_ID',
                value: process.env.AWS_ACCESS_KEY_ID,
            });
            container.env.push({
                name: 'AWS_SECRET_ACCESS_KEY',
                value: process.env.AWS_SECRET_ACCESS_KEY,
            });
        }
    });

    return new kubernetes.apps.v1.Deployment('tunnel-server', specJson, {
        provider: clusterProvider,
        dependsOn,
    });
}

function createService(dependsOn: Input<Input<Resource>[]>) {
    let specYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-service.yaml'.substring(
            1,
        ),
        'utf8',
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-service',
        ` moosicbox-tunnel-service-${$app.stage}`,
    );
    const specJson = parse(specYaml);

    return new kubernetes.core.v1.Service('tunnel-server', specJson, {
        provider: clusterProvider,
        dependsOn,
    });
}

export const image = createImage();
export const tunnelServerDeployment = createDeployment(image, []);
export const tunnelServerService = createService([tunnelServerDeployment]);

export const outputs = {
    serviceId: tunnelServerService.id,
    availableReplicas: tunnelServerDeployment.status.availableReplicas,
};
