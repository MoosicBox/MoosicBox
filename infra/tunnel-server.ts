import fs from 'node:fs';
import { parse } from 'yaml';
import { clusterProvider } from './cluster';
import { Input, Output, Resource } from '@pulumi/pulumi';

function createEcrRepo() {
    return new aws.ecr.Repository('moosicbox-tunnel-server', {
        forceDelete: true,
    });
}

type AuthToken = { userName: Output<string>; password: Output<string> };

function createImage(repo: aws.ecr.Repository, authToken: AuthToken) {
    const context = `../..`;
    return new docker.Image('tunnel-server', {
        imageName: $interpolate`${repo.repositoryUrl}`,
        build: {
            platform: 'linux/amd64',
            context: context,
            dockerfile: `${context}/packages/tunnel_server/TunnelServer.Dockerfile`,
        },
        registry: {
            server: repo.repositoryUrl,
            username: authToken.userName,
            password: authToken.password,
        },
    });
}

function createDeployment(
    image: docker.Image,
    authToken: AuthToken,
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

    const containers = specJson.spec.template.spec.containers;

    containers.forEach(
        (container: {
            env: Record<string, string>[] | undefined;
            image:
            | string
            | awsx.ecr.Image['imageUri']
            | {
                username: Output<string>;
                password: Output<string>;
                name: Output<string>;
            };
        }) => {
            container.image = {
                name: $interpolate`${image.imageName}`,
                username: $interpolate`${authToken.userName}`,
                password: $interpolate`${authToken.password}`,
            };
        },
    );

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

const repo = createEcrRepo();
const authToken = aws.ecr.getAuthorizationTokenOutput({
    registryId: repo.registryId,
});
const image = createImage(repo, authToken);
const tunnelServerDeployment = createDeployment(image, authToken, []);
// const tunnelServerService = createService([]);

export const outputs = {
    // serviceId: tunnelServerService.id,
    availableReplicas: tunnelServerDeployment.status.availableReplicas,
};
