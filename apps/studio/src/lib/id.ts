export function randomBase64Url96(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(12))
  return btoa(String.fromCharCode(...bytes))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
}

export function randomId(prefix: string): string {
  return `${prefix}:${randomBase64Url96()}`
}
