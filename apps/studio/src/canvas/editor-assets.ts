import { type Asset, AssetRecordType } from "./types/assets"
import type { AssetId } from "./types/ids"
import type { CanvasRecord, RecordId } from "./types/records"

type ReadRecord = (id: RecordId) => CanvasRecord | undefined
type WriteRecord = (record: CanvasRecord) => void

export function createEditorAssets(
  assets: Array<
    Partial<Asset> & { id?: string; type: string; typeName: "asset" }
  >,
  writeRecord: WriteRecord
): Asset[] {
  return assets.map((asset) => {
    const result = {
      id: asset.id || AssetRecordType.createId(),
      typeName: "asset",
      type: asset.type,
      props: asset.props || {},
      meta: asset.meta || {},
    } as Asset
    writeRecord(result)
    return result
  })
}

export function getEditorAsset(
  id: AssetId,
  readRecord: ReadRecord
): Asset | undefined {
  const record = readRecord(id)
  return record?.typeName === "asset" ? (record as Asset) : undefined
}

export function updateEditorAsset(
  id: AssetId,
  updates: Partial<Asset>,
  readRecord: ReadRecord,
  writeRecord: WriteRecord
): void {
  const existing = readRecord(id) as Asset | undefined
  if (!existing || existing.typeName !== "asset") return
  const updated = {
    ...existing,
    ...updates,
    props: { ...existing.props, ...(updates.props || {}) },
    meta: { ...existing.meta, ...(updates.meta || {}) },
  } as Asset
  writeRecord(updated)
}
