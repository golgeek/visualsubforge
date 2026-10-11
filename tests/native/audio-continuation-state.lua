-- State/lifecycle contract only: this does not measure submitted/captured PCM.
local mp = require 'mp'
local utils = require 'mp.utils'
local config = utils.parse_json(os.getenv('VSF_CONTINUATION_CONFIG'))
local property = 'vsf-audio-continuation-pts'
local result = {checks = {}, transitions = {}, passed = false}
local started = mp.get_time()
local phase = 'loading'
local phase_at = started
local prior, wraps, frozen = nil, 0, nil
local finished = false

local function check(ok, label)
    table.insert(result.checks, {label = label, passed = not not ok})
    return ok
end

local function finish(error)
    if finished then return end
    finished = true
    result.error = error
    result.passed = error == nil
    for _, row in ipairs(result.checks) do
        result.passed = result.passed and row.passed
    end
    local file = assert(io.open(config.output, 'w'))
    file:write(utils.format_json(result))
    file:close()
    mp.commandv('quit')
end

local function enter(next_phase)
    phase = next_phase
    phase_at = mp.get_time()
    table.insert(result.transitions, {phase = phase, seconds = phase_at - started})
end

check(mp.get_property_number(property) == nil, 'unavailable before load')
mp.commandv('loadfile', config.media)
mp.add_periodic_timer(.01, function()
    if finished then return end
    local ok, error = pcall(function()
        local now = mp.get_time()
        local pts = mp.get_property_number('audio-pts')
        if phase == 'loading' and mp.get_property('current-ao') then
            check(mp.get_property('current-ao') == config.backend, 'requested backend')
            mp.set_property_number('speed', config.speed)
            mp.set_property_number('ab-loop-a', .5)
            mp.set_property_number('ab-loop-b', .823)
            mp.commandv('seek', '.5', 'absolute+exact')
            mp.set_property_bool('pause', false)
            enter('looping')
        elseif phase == 'looping' then
            check(mp.get_property_number(property) == nil, 'unavailable while playing')
            if prior and pts and pts < prior - .1 then wraps = wraps + 1 end
            prior = pts
            if wraps >= 2 and pts and pts > .6 and pts < .7 then
                mp.set_property_bool('pause', true)
                frozen = mp.get_property_number(property)
                if config.backend == 'coreaudio' then
                    check(frozen and frozen >= .5 and frozen <= .823, 'paused continuation within source loop')
                else
                    check(frozen == nil, 'unsupported backend remains unavailable')
                end
                local written = mp.set_property_number(property, .7)
                check(not written, 'continuation is read-only')
                enter('paused')
            end
        elseif phase == 'paused' and now - phase_at > .25 then
            local current = mp.get_property_number(property)
            check(current == frozen, 'paused snapshot stable')
            check(mp.get_property_bool('pause'), 'pause retained')
            if config.backend ~= 'coreaudio' then
                finish()
                return
            end
            if not frozen then error('missing continuation') end
            mp.set_property('ab-loop-a', 'no')
            mp.set_property('ab-loop-b', 'no')
            mp.commandv('seek', tostring(frozen), 'absolute+exact')
            mp.set_property_bool('pause', false)
            enter('cleared')
        elseif phase == 'cleared' and mp.get_property_bool('eof-reached') then
            check(mp.get_property_number('time-pos', 0) >= 1.99, 'continuation reaches source EOF')
            mp.commandv('stop')
            enter('stopped')
        elseif phase == 'stopped' and not mp.get_property('current-ao') then
            check(mp.get_property_number(property) == nil, 'unavailable after stop')
            finish()
        end
        if now - started > 15 then error('watchdog in ' .. phase) end
    end)
    if not ok then finish(tostring(error)) end
end)
