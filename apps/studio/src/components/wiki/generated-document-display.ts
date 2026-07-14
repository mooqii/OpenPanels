export function countDocumentCharacters(content: string): number {
  return Array.from(content).filter((character) => !/\s/u.test(character))
    .length
}
