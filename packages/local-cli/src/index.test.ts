import { mkdtemp, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { Writable } from "node:stream"
import { afterEach, beforeEach, describe, expect, it } from "vitest"
import packageJson from "../package.json"
import { runOpenPanelsCli } from "./index"

const TINY_PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
  "base64"
)

describe("openpanels-local CLI", () => {
  let projectDir: string

  beforeEach(async () => {
    projectDir = await mkdtemp(join(tmpdir(), "openpanels-local-cli-test-"))
  })

  afterEach(async () => {
    await rm(projectDir, { recursive: true, force: true })
  })

  it("reports the CLI version", async () => {
    const output = await runCli(["--version"])

    expect(output.exitCode).toBe(0)
    expect(output.stdout).toBe(`${packageJson.version}\n`)
  })

  it("reports missing studio status as JSON", async () => {
    const output = await runCli([
      "studio",
      "status",
      "--project",
      projectDir,
      "--format",
      "json",
    ])

    expect(output.exitCode).toBe(0)
    expect(JSON.parse(output.stdout)).toMatchObject({
      ok: true,
      server: "missing",
      projectDir,
    })
  })

  it("inserts an image and reads the fallback selection", async () => {
    const imagePath = join(projectDir, "image.png")
    await writeFile(imagePath, TINY_PNG)

    const inserted = await runCli([
      "insert-image",
      "--project",
      projectDir,
      "--image",
      imagePath,
      "--format",
      "json",
    ])
    expect(inserted.exitCode).toBe(0)
    const insertedPayload = JSON.parse(inserted.stdout)

    const selection = await runCli([
      "selection",
      "--project",
      projectDir,
      "--format",
      "json",
    ])
    expect(selection.exitCode).toBe(0)
    expect(JSON.parse(selection.stdout).selection).toMatchObject({
      fallback: "last-image",
      selectedShapeIds: [insertedPayload.shapeId],
    })
  })
})

async function runCli(argv: string[]) {
  const stdout = new CaptureStream()
  const stderr = new CaptureStream()
  const exitCode = await runOpenPanelsCli(argv, { stdout, stderr })
  return { exitCode, stdout: stdout.text, stderr: stderr.text }
}

class CaptureStream extends Writable {
  text = ""

  _write(
    chunk: Buffer | string,
    _encoding: BufferEncoding,
    callback: (error?: Error | null) => void
  ) {
    this.text += chunk.toString()
    callback()
  }
}
