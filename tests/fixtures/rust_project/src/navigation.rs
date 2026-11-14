pub struct Counter { pub value: i32 }

pub fn twice(value: i32) -> i32 { value * 2 }

pub fn update(counter: Counter, delta: i32) -> i32 {
    twice(delta) + counter.value
}
