import type { MyOpenPanelsPanelKind } from "../protocol"

export class BootstrapContractError extends Error {
  constructor(message: string) {
    super(message)
    this.name = "BootstrapContractError"
  }
}

export class PanelStateContractError extends BootstrapContractError {
  constructor(kind: MyOpenPanelsPanelKind) {
    super(`Studio received malformed ${kind} panel state from the server.`)
    this.name = "PanelStateContractError"
  }
}
