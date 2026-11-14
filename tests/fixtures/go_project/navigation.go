package main

type Counter struct { Value int }

func twice(value int) int { return value * 2 }

func update(counter Counter, delta int) int {
    return twice(delta) + counter.Value
}
