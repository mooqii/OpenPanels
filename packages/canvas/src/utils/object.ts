/**
 * An alias for `Object.entries` that treats the object as a map and so preserves the type of the
 * keys and values. Unlike standard Object.entries which returns `Array<[string, unknown]>`, this maintains specific types.
 *
 * @param object - The object to get entries from
 * @returns Array of key-value pairs with preserved types
 * @example
 * ```ts
 * const user = { name: 'Alice', age: 30 }
 * const entries = objectMapEntries(user)
 * // entries is Array<['name' | 'age', string | number]>
 * ```
 * @internal
 */
export function objectMapEntries<Obj extends object>(
  object: Obj
): [keyof Obj, Obj[keyof Obj]][] {
  return Object.entries(object) as [keyof Obj, Obj[keyof Obj]][]
}
