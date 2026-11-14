class_name Player
extends Node

const Math = preload("res://math.gdscript")
var speed: float = 2.0

## Apply a delta through the helper.
func update(delta: float) -> float:
    return twice(delta) * speed

func twice(value: float) -> float:
    return Math.scale(value)

class State:
    var active: bool = true
    func enter() -> bool:
        return active
