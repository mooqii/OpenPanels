import type { Key } from "@heroui/react"
import {
  Alert,
  Button,
  ComboBox,
  Description,
  Input,
  Label,
  ListBox,
  Modal,
  Spinner,
} from "@heroui/react"
import { ImageOff, MapPinned, RefreshCw } from "lucide-react"
import { useCallback, useEffect, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { apiJson } from "../../lib/api"
import { STABLE_BACKDROP_VARIANT } from "../../lib/overlay-safety"
import type { MyOpenPanelsTransport } from "../../types"

export const V2EX_SKILL_ID = "release-v2ex"

export interface V2exNode {
  id: number
  name: string
  stars: number
  title: string
  titleAlternative: string
  topics: number
}

let cachedNodes: Promise<V2exNode[]> | null = null

function loadV2exNodes(apiBase: string, refresh = false) {
  if (refresh) cachedNodes = null
  cachedNodes ??= apiJson<{ nodes: V2exNode[] }>(
    apiBase,
    "/api/publishing/v2ex/nodes"
  ).then(({ nodes }) => nodes)
  return cachedNodes.catch((error) => {
    cachedNodes = null
    throw error
  })
}

export function V2exNodeSelectionDialog({
  imageCount,
  initialNode,
  isOpen,
  onCancel,
  onConfirm,
  transport,
}: {
  imageCount: number
  initialNode: V2exNode | null
  isOpen: boolean
  onCancel: () => void
  onConfirm: (node: V2exNode) => void
  transport: MyOpenPanelsTransport
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const [nodes, setNodes] = useState<V2exNode[]>([])
  const [selectedKey, setSelectedKey] = useState<Key | null>(
    initialNode?.name ?? null
  )
  const [isLoading, setIsLoading] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)

  const load = useCallback(
    (refresh = false) => {
      setIsLoading(true)
      setLoadError(null)
      loadV2exNodes(transport.apiBase, refresh)
        .then(setNodes)
        .catch((cause) =>
          setLoadError(String((cause as Error)?.message || cause))
        )
        .finally(() => setIsLoading(false))
    },
    [transport.apiBase]
  )

  useEffect(() => {
    if (!isOpen) return
    setSelectedKey(initialNode?.name ?? null)
    load()
  }, [initialNode, isOpen, load])

  const selectedNode =
    nodes.find((node) => node.name === String(selectedKey ?? "")) ??
    (isLoading ? initialNode : null)

  return (
    <Modal.Backdrop
      isOpen={isOpen}
      onOpenChange={(open) => !open && onCancel()}
      variant={STABLE_BACKDROP_VARIANT}
    >
      <Modal.Container placement="center" size="md">
        <Modal.Dialog className="op-v2ex-node-dialog">
          <Modal.CloseTrigger aria-label={t`Close`} />
          <Modal.Header>
            <Modal.Icon className="text-accent">
              <MapPinned size={20} />
            </Modal.Icon>
            <div>
              <Modal.Heading>{t`Choose a V2EX node`}</Modal.Heading>
              <p className="op-v2ex-node-dialog__intro">
                {t`The selected node is fixed for this publishing attempt.`}
              </p>
            </div>
          </Modal.Header>
          <Modal.Body className="op-v2ex-node-dialog__body">
            {imageCount > 0 ? (
              <Alert status="warning">
                <Alert.Indicator>
                  <ImageOff size={16} />
                </Alert.Indicator>
                <Alert.Content>
                  <Alert.Title>{t`Images will not be published`}</Alert.Title>
                  <Alert.Description>
                    {locale === "zh-CN"
                      ? `${imageCount.toLocaleString(locale)} 张正文或封面图片会被过滤，仅发布标题和文字正文。`
                      : `${imageCount.toLocaleString(locale)} article or cover images will be removed. Only the title and text body will be published.`}
                  </Alert.Description>
                </Alert.Content>
              </Alert>
            ) : null}

            {loadError ? (
              <Alert status="danger">
                <Alert.Indicator />
                <Alert.Content>
                  <Alert.Title>{t`Could not load V2EX nodes`}</Alert.Title>
                  <Alert.Description>{loadError}</Alert.Description>
                </Alert.Content>
                <Button
                  aria-label={t`Retry`}
                  isIconOnly
                  onPress={() => load(true)}
                  size="sm"
                  variant="ghost"
                >
                  <RefreshCw size={15} />
                </Button>
              </Alert>
            ) : null}

            <ComboBox
              allowsEmptyCollection
              className="op-v2ex-node-dialog__picker"
              defaultFilter={(textValue, inputValue) =>
                !inputValue ||
                textValue
                  .toLocaleLowerCase()
                  .includes(inputValue.toLocaleLowerCase())
              }
              isDisabled={isLoading || Boolean(loadError)}
              onSelectionChange={setSelectedKey}
              selectedKey={selectedKey}
            >
              <Label>{t`Publishing node`}</Label>
              <ComboBox.InputGroup>
                <Input placeholder={t`Search node name or short name`} />
                <ComboBox.Trigger />
              </ComboBox.InputGroup>
              <ComboBox.Popover>
                <ListBox>
                  {nodes.map((node) => (
                    <ListBox.Item
                      id={node.name}
                      key={node.name}
                      textValue={`${node.title} /go/${node.name}`}
                    >
                      <div className="op-v2ex-node-dialog__node">
                        <Label>{node.title}</Label>
                        <Description>
                          /go/{node.name} · {node.topics.toLocaleString(locale)}{" "}
                          {t`topics`}
                        </Description>
                      </div>
                      <ListBox.ItemIndicator />
                    </ListBox.Item>
                  ))}
                </ListBox>
              </ComboBox.Popover>
            </ComboBox>

            {isLoading ? (
              <div className="op-v2ex-node-dialog__loading">
                <Spinner size="sm" />
                {t`Loading V2EX nodes`}
              </div>
            ) : null}
          </Modal.Body>
          <Modal.Footer>
            <Button onPress={onCancel} variant="tertiary">
              {t`Cancel`}
            </Button>
            <Button
              isDisabled={!selectedNode || isLoading || Boolean(loadError)}
              onPress={() => selectedNode && onConfirm(selectedNode)}
              variant="primary"
            >
              {t`Continue`}
            </Button>
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}
