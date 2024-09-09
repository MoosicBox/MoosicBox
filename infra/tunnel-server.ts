import { parse } from 'yaml';
import fs from 'node:fs';
import { clusterProvider } from './cluster';
import { certManagers } from './cert-manager';

function createDeployment() {
    let tunnelServerDeploymentSpecYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-deployment.yaml'.substring(
            1,
        ),
        'utf8',
    );
    tunnelServerDeploymentSpecYaml = tunnelServerDeploymentSpecYaml.replaceAll(
        ' moosicbox-tunnel-server',
        ` moosicbox-tunnel-server-${$app.stage}`,
    );
    tunnelServerDeploymentSpecYaml = tunnelServerDeploymentSpecYaml.replaceAll(
        ' moosicbox-tunnel-service',
        ` moosicbox-tunnel-service-${$app.stage}`,
    );
    const tunnelServerDeploymentSpec = parse(tunnelServerDeploymentSpecYaml);

    return new kubernetes.apps.v1.Deployment(
        'tunnel-server',
        tunnelServerDeploymentSpec,
        { provider: clusterProvider, dependsOn: [certManagers] },
    );
}

function createService() {
    let tunnelServerServiceSpecYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-service.yaml'.substring(
            1,
        ),
        'utf8',
    );
    tunnelServerServiceSpecYaml = tunnelServerServiceSpecYaml.replaceAll(
        ' moosicbox-tunnel-service',
        ` moosicbox-tunnel-service-${$app.stage}`,
    );
    const tunnelServerServiceSpec = parse(tunnelServerServiceSpecYaml);

    return new kubernetes.core.v1.Service(
        'tunnel-server',
        tunnelServerServiceSpec,
        { provider: clusterProvider, dependsOn: [certManagers] },
    );
}

const tunnelServerDeployment = createDeployment();
const tunnelServerService = createService();

export const outputs = {
    serviceId: tunnelServerService.id,
    availableReplicas: tunnelServerDeployment.status.availableReplicas,
};
