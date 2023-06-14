// makes a Record<K, V> a union of all its variants
// based on the serde tagged enum pattern
export type TaggedEnum<T> = {
  [K in keyof T]: { type: K; data: T[K] };
}[keyof T];
