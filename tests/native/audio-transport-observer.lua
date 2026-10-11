-- Opt-in transport diagnostic. Python owns the child lifetime and validates the
-- trace with the same assertions as the POSIX observer. No application changes.
local utils = require "mp.utils"
local config = assert(utils.parse_json(assert(os.getenv("VSF_AUDIO_TRANSPORT_CONFIG"))))
local function array() return utils.parse_json("[]") end
local trace = {
    start = config.start, ["end"] = config["end"], looping = config.looping,
    media = config.media, samples = array(), transitions = array(),
    requested_interval_seconds = 0.02,
}
local properties = {"time-pos", "audio-pts", "remaining-ab-loops", "seeking",
                    "eof-reached", "pause", "current-ao", "vsf-audio-clock-version"}
local deadline = mp.get_time() + (config.capture_ready and 10 or 5)
local began, timer
local phase, action_index = "initial", 1
local finished = false

local function command(args)
    local result, err
    if args[1] == "set_property" then
        result, err = mp.set_property_native(args[2], args[3])
    else
        result, err = mp.command_native(args)
    end
    if err then error(args[1] .. ": " .. err) end
end

local function finish(err)
    if finished then return end
    finished = true
    if timer then timer:kill() end
    if err then trace.error = tostring(err) end
    local file, failure = io.open(config.output, "wb")
    if not file then
        mp.msg.error(failure)
        mp.commandv("quit", "1")
        return
    end
    local json = assert(utils.format_json(trace))
    assert(file:write(json, "\n"))
    assert(file:close())
    mp.commandv("quit", err and "1" or "0")
end

local function tick()
    if not began then
        local output = mp.get_property_native("current-ao")
        local capture_ready = not config.capture_ready
        if config.capture_ready then
            local ready = io.open(config.capture_ready, "rb")
            if ready then
                capture_ready = ready:read("*a") == "READY windows-process-loopback\n"
                ready:close()
            end
        end
        if not mp.get_property_native("duration") or not output or not capture_ready then
            if mp.get_time() >= deadline then error("media/audio output/capture did not become ready") end
            return
        end
        trace.audio_output = output
        if config.capture_clock then
            -- The helper's QPC read falls inside this client-clock interval.
            -- Preserve the whole bracket; never pretend the subprocess delay
            -- is a known clock offset or a presentation measurement.
            local before = mp.get_time()
            local reply = mp.command_native({name = "subprocess",
                args = {config.capture_clock, "--clock"}, capture_stdout = true})
            local after = mp.get_time()
            if not reply or reply.status ~= 0 then error("capture clock calibration failed") end
            local clock = assert(utils.parse_json(reply.stdout))
            trace.capture_clock_calibration = {mp_before = before, mp_after = after,
                                              qpc_ticks = clock.ticks, qpc_frequency = clock.frequency}
        end
        command({"set_property", "speed", 1})
        if config.looping then
            command({"set_property", "ab-loop-a", config.start})
            command({"set_property", "ab-loop-b", config["end"]})
        end
        command({"seek", config.start, "absolute+exact"})
        command({"set_property", "pause", false})
        began = mp.get_time()
        trace.sample_origin_mp_time_seconds = began
        return
    end
    local elapsed = mp.get_time() - began
    if elapsed >= config.duration then finish(); return end
    local action = config.actions[action_index]
    while action and elapsed >= action[1] do
        phase = action[2]
        local transition = {phase = phase, scheduled_seconds = action[1],
                            before_seconds = mp.get_time() - began, commands = action[3]}
        -- Retain partial transactions when a command fails.
        table.insert(trace.transitions, transition)
        for _, args in ipairs(action[3]) do
            if args[1] == "diagnostic-clear-loop-at-audio-pts"
                or args[1] == "diagnostic-refresh-at-audio-pts" then
                local raw = mp.get_property_native("audio-pts")
                if type(raw) ~= "number" then error("audio-pts unavailable at transition") end
                transition.raw_audio_pts = raw
                transition.resolved_seek_seconds = raw
                if args[1] == "diagnostic-clear-loop-at-audio-pts" then
                    command({"set_property", "ab-loop-a", "no"})
                    command({"set_property", "ab-loop-b", "no"})
                end
                command({"seek", raw, "absolute+exact"})
            else
                command(args)
            end
        end
        transition.after_seconds = mp.get_time() - began
        action_index = action_index + 1
        action = config.actions[action_index]
    end
    local sample = {elapsed_seconds = mp.get_time() - began, phase = phase}
    for _, prop in ipairs(properties) do sample[prop] = mp.get_property_native(prop) end
    sample.sample_complete_elapsed_seconds = mp.get_time() - began
    table.insert(trace.samples, sample)
end

timer = mp.add_periodic_timer(0.02, function()
    local ok, err = pcall(tick)
    if not ok then finish(err) end
end)
