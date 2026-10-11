-- Event-driven EOF diagnostic. No periodic timer or playback-clock polling.
local utils = require "mp.utils"
local config = assert(utils.parse_json(assert(os.getenv("VSF_AUDIO_EOF_CONFIG"))))
local result = { events = utils.parse_json("[]"), mode = config.mode }
local started, finished, action_done, paused_by_test = nil, false, false, false

local function record(kind, extra)
    local event = extra or {}
    event.kind = kind
    event.seconds = started and mp.get_time() - started or 0
    table.insert(result.events, event)
end

local function finish(err)
    if finished then return end
    finished = true
    result.error = err
    local file = assert(io.open(config.output, "wb"))
    assert(file:write(assert(utils.format_json(result)), "\n"))
    assert(file:close())
    mp.commandv("quit", err and "1" or "0")
end

local function guarded(fn)
    return function(...)
        if finished then return end
        local ok, err = pcall(fn, ...)
        if not ok then finish(tostring(err)) end
    end
end

local function command(args)
    local _, err = mp.command_native(args)
    if err then error(err) end
end

mp.observe_property("pause", "bool", guarded(function(_, paused)
    record("pause", { value = paused })
    if not started or not paused or paused_by_test then return end
    if not mp.get_property_native("eof-reached") then return end
    if config.mode ~= "ordinary" and not action_done then
        error("EOF preceded scheduled action")
    end
    record("completed", { eof = true, backend = mp.get_property_native("current-ao") })
    finish()
end))

mp.register_event("playback-restart", guarded(function()
    record("playback-restart")
    if started then return end
    started = mp.get_time()
    result.backend = mp.get_property_native("current-ao")
    record("started")
    if config.mode ~= "ordinary" then
        mp.add_timeout(config.action_seconds, guarded(function()
            record("action", { position = mp.get_property_native("audio-pts") })
            if config.mode == "pause" or config.mode == "pause-seek" or config.mode == "pause-reload" then
                paused_by_test = true
                command({ "set", "pause", "yes" })
                local expected_source = config.media
                if config.mode == "pause-seek" then
                    command({ "seek", "0.5", "absolute+exact" })
                    record("paused-reset")
                elseif config.mode == "pause-reload" then
                    expected_source = config.replacement
                    command({ "loadfile", expected_source, "replace" })
                    record("paused-reset")
                end
                mp.add_timeout(0.25, guarded(function()
                    record("resume", {
                        position = mp.get_property_native("audio-pts"),
                        transport_position = mp.get_property_native("time-pos"),
                        paused = mp.get_property_native("pause"),
                        duration = mp.get_property_native("duration"),
                        source_path = mp.get_property_native("path"),
                    })
                    command({ "set", "pause", "no" })
                    paused_by_test = false
                    action_done = true
                end))
            elseif config.mode == "seek" then
                command({ "seek", "0.5", "absolute+exact" })
                action_done = true
            elseif config.mode == "reload" then
                command({ "loadfile", config.media, "replace" })
                action_done = true
            else
                error("Unknown mode")
            end
        end))
    end
end))

-- The only general timer is a failure deadline. It never reads the clock.
mp.add_timeout(6, function() finish("No paused EOF event before watchdog") end)
