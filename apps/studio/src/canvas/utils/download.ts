export function downloadUrlAsFile(sourceUrl: string, fileName: string): void {
  const link = document.createElement("a")
  link.download = fileName
  link.href = sourceUrl
  document.body.appendChild(link)

  try {
    link.click()
  } finally {
    link.remove()
  }
}
