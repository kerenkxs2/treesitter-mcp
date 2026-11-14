local Math = require("math_helpers")
local Player = { speed = 2 }

local function twice(value)
    return Math.scale(value)
end

-- Apply a delta through the helper.
function Player:update(delta)
    return twice(delta) * self.speed
end

Player.reset = function(self)
    self.speed = 2
end

local Tools = {
    clamp = function(value)
        return math.max(value, 0)
    end,
}

return Player
