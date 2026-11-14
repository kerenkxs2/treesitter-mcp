export interface Counter { value: number }

export function twice(value: number): number {
    return value * 2;
}

export function update(counter: Counter, delta: number): number {
    return twice(delta) + counter.value;
}
