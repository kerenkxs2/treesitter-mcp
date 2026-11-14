package receivers

func twice(value int) int { return value * 2 }

// buildPodman intentionally has its receiver type in a different file.
func (p Planner) buildPodman(value int) int {
    return twice(value) + p.Base
}

func (p *Planner) Reset() { p.Base = 0 }

func (b *Box[T]) Value() T { return b.item }

func run() int { return Planner{Base: 1}.buildPodman(2) }
