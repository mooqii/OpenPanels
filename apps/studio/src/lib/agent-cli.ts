import type { LocalCliInfo, ModelGatewaySettings } from "../types"

export function hasUsableAgentCli(
  settings: ModelGatewaySettings,
  localClis: LocalCliInfo[]
): boolean {
  const enabledProviderIds = new Set(settings.localCli.enabledProviderIds)
  return localClis.some(
    (cli) =>
      enabledProviderIds.has(cli.id) &&
      cli.available &&
      cli.authStatus !== "missing" &&
      !cli.authMessage &&
      !cli.diagnostic
  )
}
