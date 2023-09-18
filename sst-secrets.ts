import { SSMClient, GetParameterCommand } from '@aws-sdk/client-ssm';

function formatSstSecretParameterName(
    app: string,
    name: string,
    stage: string,
) {
    return `/sst/${app}/${stage}/Secret/${name}/value`;
}

async function fetchParameterValue(client: SSMClient, name: string) {
    const response = await client.send(
        new GetParameterCommand({
            Name: name,
            WithDecryption: true,
        }),
    );

    if (!response.Parameter?.Value) {
        throw Error(`Parameter ${name} does not have a valid value`);
    }

    return response.Parameter!.Value!;
}

export async function fetchSstSecret(
    client: SSMClient,
    app: string,
    name: string,
    stage: string,
    checkFallback = true,
): Promise<string> {
    try {
        try {
            return await fetchParameterValue(
                client,
                formatSstSecretParameterName(app, name, stage),
            );
        } catch (e) {
            if (!checkFallback)
                throw Error(`No value for secret ${name} for stage ${stage}`);

            return await fetchParameterValue(
                client,
                formatSstSecretParameterName(app, name, '.fallback'),
            );
        }
    } catch (e) {
        throw Error(`Failed to fetch fallback SST secret ${name}`);
    }
}
