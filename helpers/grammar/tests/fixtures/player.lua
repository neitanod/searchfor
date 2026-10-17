local Player = {}

function Player:update(dt)
  self.x = self.x + dt -- needle
end

local function helper()
  return "needle"
end
