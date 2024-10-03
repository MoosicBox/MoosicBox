export type PartialBy<T, K extends keyof T> = Omit<T, K> & Partial<Pick<T, K>>;

export type Entries<T, K extends keyof T = keyof T> = (K extends unknown
    ? [K, T[K]]
    : never)[];
