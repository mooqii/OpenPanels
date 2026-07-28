import {
  Alert,
  Button,
  Chip,
  Input,
  Label,
  Modal,
  Spinner,
} from "@heroui/react"
import {
  AlertTriangle,
  CheckCircle2,
  Clipboard,
  Clock3,
  ExternalLink,
  KeyRound,
  Network,
} from "lucide-react"
import { useEffect, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { apiJson } from "../../lib/api"
import type { MyOpenPanelsTransport } from "../../types"

export const WECHAT_API_SKILL_ID = "release-wechat-official-account"

type CheckStatus = "failed" | "passed" | "pending"

export interface WechatConfigurationStatus {
  appId: string | null
  checks: {
    credentials: CheckStatus
    draftApi: CheckStatus
    ipAllowlist: CheckStatus
    publicIp: CheckStatus
  }
  configured: boolean
  publicIp: string | null
  ready: boolean
  reasonCode: string | null
  saved: boolean
  summary?: string
  validatedAt: string | null
  validatedPublicIp: string | null
  wechatObservedIp: string | null
}

export function loadWechatConfiguration(apiBase: string) {
  return apiJson<WechatConfigurationStatus>(
    apiBase,
    "/api/publishing/wechat/configuration"
  )
}

export function WechatApiConfigurationDialog({
  initialStatus,
  isOpen,
  issueReasonCode,
  onCancel,
  onReady,
  transport,
}: {
  initialStatus: WechatConfigurationStatus | null
  isOpen: boolean
  issueReasonCode: string | null
  onCancel: () => void
  onReady: () => void
  transport: MyOpenPanelsTransport
}) {
  const { t } = useMyOpenPanelsI18n()
  const [status, setStatus] = useState<WechatConfigurationStatus | null>(
    initialStatus
  )
  const [appId, setAppId] = useState("")
  const [appSecret, setAppSecret] = useState("")
  const [isLoading, setIsLoading] = useState(false)
  const [isSaving, setIsSaving] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (!isOpen) return
    let cancelled = false
    setStatus(initialStatus)
    setAppId(initialStatus?.appId ?? "")
    setAppSecret("")
    setCopied(false)
    setLoadError(null)
    setIsLoading(true)
    loadWechatConfiguration(transport.apiBase)
      .then((next) => {
        if (cancelled) return
        setStatus(next)
        setAppId(next.appId ?? "")
      })
      .catch((cause) => {
        if (!cancelled) {
          setLoadError(String((cause as Error)?.message || cause))
        }
      })
      .finally(() => {
        if (!cancelled) setIsLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [initialStatus, isOpen, transport.apiBase])

  async function validateAndSave() {
    setIsSaving(true)
    setLoadError(null)
    try {
      const next = await apiJson<WechatConfigurationStatus>(
        transport.apiBase,
        "/api/publishing/wechat/configuration",
        {
          body: JSON.stringify({
            appId,
            appSecret: appSecret.trim() || null,
          }),
          headers: { "content-type": "application/json" },
          method: "PUT",
        }
      )
      setStatus(next)
      if (next.ready && next.saved) {
        setAppSecret("")
        onReady()
      }
    } catch (cause) {
      setLoadError(String((cause as Error)?.message || cause))
    } finally {
      setIsSaving(false)
    }
  }

  const activeReason = status?.summary
    ? status.reasonCode
    : (issueReasonCode ?? status?.reasonCode)
  const issue = configurationIssue(activeReason, status?.summary, t)
  const canValidate = Boolean(
    appId.trim() && (appSecret.trim() || status?.configured)
  )
  const credentialStatus = effectiveCheckStatus(
    "credentials",
    status?.checks.credentials,
    activeReason
  )
  const publicIpStatus = effectiveCheckStatus(
    "publicIp",
    status?.checks.publicIp,
    activeReason
  )
  const allowlistStatus = effectiveCheckStatus(
    "ipAllowlist",
    status?.checks.ipAllowlist,
    activeReason
  )
  const draftApiStatus = effectiveCheckStatus(
    "draftApi",
    status?.checks.draftApi,
    activeReason
  )
  const wechatConsoleUrl = appId.trim()
    ? `https://developers.weixin.qq.com/console/product/mp/${encodeURIComponent(appId.trim())}?tab1=basicInfo&tab2=wxAccount`
    : "https://developers.weixin.qq.com/console/product/mp"
  const observedIpDiffers = Boolean(
    status?.wechatObservedIp &&
      status.publicIp &&
      status.wechatObservedIp !== status.publicIp
  )

  return (
    <Modal.Backdrop
      isOpen={isOpen}
      onOpenChange={(open) => !open && onCancel()}
      variant="blur"
    >
      <Modal.Container placement="center" size="lg">
        <Modal.Dialog className="op-wechat-config">
          <Modal.CloseTrigger aria-label={t`Close`} />
          <Modal.Header>
            <Modal.Icon className="text-accent">
              <KeyRound size={20} />
            </Modal.Icon>
            <div className="op-wechat-config__intro">
              <Modal.Heading>{t`WeChat Official Account (API)`}</Modal.Heading>
              <p className="op-wechat-config__intro-copy">
                {t`Validate the account before creating a draft.`}
              </p>
            </div>
          </Modal.Header>
          <Modal.Body className="op-wechat-config__body">
            {isLoading ? (
              <div className="op-wechat-config__loading">
                <Spinner size="sm" />
                {t`Checking configuration`}
              </div>
            ) : null}

            {issue ? (
              <Alert status="warning">
                <Alert.Indicator />
                <Alert.Content>
                  <Alert.Title>{issue.title}</Alert.Title>
                  <Alert.Description>{issue.description}</Alert.Description>
                </Alert.Content>
              </Alert>
            ) : null}

            <section className="op-wechat-config__section">
              <div className="op-wechat-config__section-heading">
                <div>
                  <h3>{t`Account credentials`}</h3>
                  <p className="op-wechat-config__section-copy">
                    {t`Open the WeChat Developer Platform page for this AppID to manage the Official Account credentials.`}
                  </p>
                </div>
                <a href={wechatConsoleUrl} rel="noreferrer" target="_blank">
                  {t`Open platform`}
                  <ExternalLink size={13} />
                </a>
              </div>
              <div className="op-wechat-config__fields">
                <div className="op-wechat-config__field">
                  <Label
                    className="op-wechat-config__field-label"
                    htmlFor="op-wechat-app-id"
                  >
                    AppID
                  </Label>
                  <Input
                    autoComplete="off"
                    fullWidth
                    id="op-wechat-app-id"
                    onChange={(event) => setAppId(event.currentTarget.value)}
                    placeholder="wx..."
                    value={appId}
                  />
                </div>
                <div className="op-wechat-config__field">
                  <Label
                    className="op-wechat-config__field-label"
                    htmlFor="op-wechat-app-secret"
                  >
                    AppSecret
                  </Label>
                  <Input
                    autoComplete="new-password"
                    fullWidth
                    id="op-wechat-app-secret"
                    onChange={(event) =>
                      setAppSecret(event.currentTarget.value)
                    }
                    placeholder={
                      status?.configured
                        ? t`Saved; leave blank to keep it`
                        : t`Enter AppSecret`
                    }
                    type="password"
                    value={appSecret}
                  />
                </div>
              </div>
            </section>

            <section className="op-wechat-config__section">
              <div className="op-wechat-config__section-heading">
                <div>
                  <h3>{t`IP allowlist`}</h3>
                  <p className="op-wechat-config__section-copy">
                    {t`On the same Official Account page, add the Studio server address to the IP allowlist.`}
                  </p>
                </div>
              </div>
              <div className="op-wechat-config__ip">
                <Network className="op-wechat-config__ip-icon" size={17} />
                <div className="op-wechat-config__ip-content">
                  <span>{t`Current public IP`}</span>
                  <code>{status?.publicIp ?? t`Unavailable`}</code>
                </div>
                <Button
                  aria-label={t`Copy current public IP`}
                  isDisabled={!status?.publicIp}
                  isIconOnly
                  onPress={() => {
                    if (!status?.publicIp) return
                    navigator.clipboard
                      .writeText(status.publicIp)
                      .then(() => setCopied(true))
                      .catch(() => setCopied(false))
                  }}
                  size="sm"
                  variant="ghost"
                >
                  {copied ? (
                    <CheckCircle2 size={15} />
                  ) : (
                    <Clipboard size={15} />
                  )}
                </Button>
              </div>
              {status?.wechatObservedIp ? (
                <div className="op-wechat-config__ip">
                  <Network className="op-wechat-config__ip-icon" size={17} />
                  <div className="op-wechat-config__ip-content">
                    <span>{t`IP observed by WeChat API`}</span>
                    <code>{status.wechatObservedIp}</code>
                  </div>
                </div>
              ) : null}
              {observedIpDiffers ? (
                <Alert status="danger">
                  <Alert.Indicator />
                  <Alert.Content>
                    <Alert.Title>{t`The egress IP addresses differ`}</Alert.Title>
                    <Alert.Description>
                      {t`Add the IP observed by WeChat to the allowlist too, and check the Studio host VPN or proxy routing.`}
                    </Alert.Description>
                  </Alert.Content>
                </Alert>
              ) : null}
              <p className="op-wechat-config__section-copy">
                {t`After saving the allowlist, wait briefly and validate again. The IP observed by WeChat is authoritative.`}
              </p>
            </section>

            <div className="op-wechat-config__checks">
              <ConfigurationCheck
                label={t`AppID and AppSecret`}
                status={credentialStatus}
                statusLabel={checkStatusLabel(credentialStatus, t)}
              />
              <ConfigurationCheck
                label={t`Public IP detected`}
                status={publicIpStatus}
                statusLabel={checkStatusLabel(publicIpStatus, t)}
              />
              <ConfigurationCheck
                label={t`Current IP allowlist`}
                status={allowlistStatus}
                statusLabel={checkStatusLabel(allowlistStatus, t)}
              />
              <ConfigurationCheck
                label={t`Draft API permission`}
                status={draftApiStatus}
                statusLabel={checkStatusLabel(draftApiStatus, t)}
              />
            </div>

            {loadError ? (
              <Alert status="danger">
                <Alert.Indicator />
                <Alert.Content>
                  <Alert.Title>{t`Validation failed`}</Alert.Title>
                  <Alert.Description>{loadError}</Alert.Description>
                </Alert.Content>
              </Alert>
            ) : null}
          </Modal.Body>
          <Modal.Footer>
            <Button isDisabled={isSaving} onPress={onCancel} variant="tertiary">
              {t`Cancel`}
            </Button>
            <Button
              isDisabled={!canValidate || isLoading}
              isPending={isSaving}
              onPress={validateAndSave}
            >
              <CheckCircle2 size={15} />
              {t`Validate and save`}
            </Button>
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}

function ConfigurationCheck({
  label,
  status,
  statusLabel,
}: {
  label: string
  status: CheckStatus
  statusLabel: string
}) {
  return (
    <div className="op-wechat-config__check" data-status={status}>
      {status === "passed" ? (
        <CheckCircle2
          aria-hidden
          className="op-wechat-config__check-icon"
          size={15}
        />
      ) : status === "failed" ? (
        <AlertTriangle
          aria-hidden
          className="op-wechat-config__check-icon"
          size={15}
        />
      ) : (
        <Clock3
          aria-hidden
          className="op-wechat-config__check-icon"
          size={15}
        />
      )}
      <span>{label}</span>
      <Chip
        color={
          status === "passed"
            ? "success"
            : status === "failed"
              ? "danger"
              : "default"
        }
        size="sm"
        variant="soft"
      >
        {statusLabel}
      </Chip>
    </div>
  )
}

function checkStatusLabel(
  status: CheckStatus,
  t: (value: TemplateStringsArray) => string
) {
  if (status === "passed") return t`Passed`
  if (status === "failed") return t`Failed`
  return t`Pending validation`
}

function effectiveCheckStatus(
  check: keyof WechatConfigurationStatus["checks"],
  status: CheckStatus | undefined,
  reasonCode: string | null | undefined
): CheckStatus {
  if (
    check === "credentials" &&
    [
      "wechat_app_id_missing",
      "wechat_app_secret_missing",
      "wechat_credentials_missing",
      "wechat_credentials_rejected",
    ].includes(reasonCode ?? "")
  ) {
    return "failed"
  }
  if (check === "publicIp" && reasonCode === "wechat_public_ip_unavailable") {
    return "failed"
  }
  if (check === "ipAllowlist" && reasonCode === "wechat_ip_not_allowed") {
    return "failed"
  }
  if (check === "draftApi" && reasonCode === "wechat_api_unauthorized") {
    return "failed"
  }
  return status ?? "pending"
}

function configurationIssue(
  reasonCode: string | null | undefined,
  summary: string | undefined,
  t: (value: TemplateStringsArray) => string
) {
  if (!reasonCode) return null
  if (reasonCode === "wechat_credentials_missing") {
    return {
      description: t`Enter the AppID and AppSecret from Basic Configuration.`,
      title: t`Account credentials are required`,
    }
  }
  if (
    reasonCode === "wechat_credentials_rejected" ||
    reasonCode === "wechat_app_id_missing" ||
    reasonCode === "wechat_app_secret_missing"
  ) {
    return {
      description: t`Check the AppID and reset or copy the AppSecret again.`,
      title: t`Account credentials were rejected`,
    }
  }
  if (
    reasonCode === "wechat_ip_not_allowed" ||
    reasonCode === "wechat_configuration_validation_required"
  ) {
    return {
      description: t`Add the IP shown below to the IP allowlist, then validate again.`,
      title: t`Current IP is not validated`,
    }
  }
  if (reasonCode === "wechat_api_unauthorized") {
    return {
      description: t`Confirm that this account has draft-management API permission.`,
      title: t`Draft API permission is unavailable`,
    }
  }
  return {
    description: summary || t`Review the settings below and validate again.`,
    title: t`WeChat API configuration needs attention`,
  }
}
