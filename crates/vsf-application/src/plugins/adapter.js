// Evaluated inside the bounded helper. Rust appends the single invocation.
function pluginAdapter(input) {
  const evaluate = (0, eval);
  const original = input.cues;
  const state = original.map((cue) => ({ ...cue }));
  const byId = new Map(state.map((cue, index) => [cue.id, index]));
  const scope = new Set(input.scope);
  const selected = new Set(input.context.selection);
  // Dense captured indexes need an array, not the engine's slower numeric Map.
  const wrappers = [];
  const identity = new WeakMap();
  const findings = [];
  const logs = [];
  let logBytes = 0;
  let poison = null;
  let writable = false;
  const fail = (message) => {
    poison ??= message;
    throw new Error(message);
  };
  const checkedString = (value, limit, label) => {
    if (typeof value !== 'string' || value.length > limit)
      fail(`${label} must be a bounded string`);
    return value;
  };
  const indexOf = (id) => {
    if (typeof id !== 'string' || !byId.has(id)) fail('Unknown cue identity');
    return byId.get(id);
  };
  const set = (index, field, value) => {
    if (!writable) fail('Checker and initialization code cannot change cues');
    if (field === 'text') checkedString(value, 16 * 1024 * 1024, 'Cue text');
    else if (
      !Number.isSafeInteger(value) ||
      value < 0 ||
      value >
        (input.package.manifest.api === 'vss-1' ? 2147483647 : 35999999999)
    )
      fail('Cue times must be exact supported integer milliseconds');
    state[index][field] = value;
  };
  // Deliberately mirrors the legacy tag-removal convention, not VSF reading
  // projection: retain CRLF/entities/ASS escapes; unclosed tags stay literal.
  const stripped = (text) => {
    let plain = '',
      start = 0;
    for (let i = 0; i < text.length; ++i) {
      const ass =
        text[i] === '{' && text[i + 1] === '\\' && i + 3 < text.length;
      if (!ass && text[i] !== '<') continue;
      const end = text.indexOf(ass ? '}' : '>', i + 1);
      // The legacy scanner stops at the first unclosed opener, even if its
      // remainder contains another otherwise recognizable tag.
      if (end < 0) break;
      plain += text.slice(start, i);
      start = end + 1;
      i = end;
    }
    return plain + text.slice(start);
  };
  // Share accessor functions/descriptors instead of allocating new closures
  // for each cue. Dense lookup above addresses the measured numeric-Map cost.
  // Properties remain own/enumerable/nonconfigurable on each stable wrapper.
  const legacyProperties = {};
  for (const [name, field] of [
    ['Start', 'startMs'],
    ['Stop', 'endMs'],
    ['Text', 'text'],
  ]) {
    legacyProperties[name] = {
      enumerable: true,
      get() {
        const index = identity.get(this);
        if (field !== 'text') return state[index][field];
        const text = state[index].text;
        // Most cues are one line. Avoid constructing/running a RegExp when
        // there is no newline to normalize; literal ASS escapes stay literal.
        return text.includes('\n') ? text.replace(/\r?\n/g, '\r\n') : text;
      },
      set(value) {
        set(
          identity.get(this),
          field,
          field === 'text' &&
            typeof value === 'string' &&
            value.includes('\r\n')
            ? value.replace(/\r\n/g, '\n')
            : value,
        );
      },
    };
  }
  Object.assign(legacyProperties, {
    Index: {
      enumerable: true,
      get() {
        return identity.get(this) + 1;
      },
      set: () => {
        fail('Cue Index is read-only');
      },
    },
    StrippedText: {
      enumerable: true,
      get() {
        return stripped(this.Text);
      },
      set(value) {
        this.Text = value;
      },
    },
  });
  const legacy = (index) => {
    if (index < 0 || index >= state.length) return null;
    if (wrappers[index]) return wrappers[index];
    const result = Object.defineProperties({}, legacyProperties);
    Object.preventExtensions(result);
    identity.set(result, index);
    wrappers[index] = result;
    return result;
  };
  const neighbor = (cue, offset) => {
    if (cue === null) return null;
    if (!identity.has(cue)) fail('Expected a captured VSS subtitle wrapper');
    return legacy(identity.get(cue) + offset);
  };
  const expose = (name, value) =>
    Object.defineProperty(globalThis, name, {
      value,
      configurable: false,
      writable: false,
    });
  const supported = (name, value) =>
    new Proxy(Object.freeze(value), {
      get(target, property, receiver) {
        if (typeof property === 'symbol' || Reflect.has(target, property))
          return Reflect.get(target, property, receiver);
        return fail(
          `${name}.${property} is unavailable in this compatibility subset`,
        );
      },
      set: () => fail(`${name} is read-only`),
    });
  const sceneData = () =>
    input.context.scenes ?? fail('Completed scene metadata is unavailable');
  const sceneIndex = (time, previous) => {
    if (!Number.isSafeInteger(time))
      fail('Scene lookup requires integer milliseconds');
    const cuts = sceneData().cutsMs;
    let low = 0,
      high = cuts.length;
    while (low < high) {
      const mid = (low + high) >>> 1;
      if (cuts[mid] < time || (previous && cuts[mid] === time)) low = mid + 1;
      else high = mid;
    }
    return previous ? low - 1 : low;
  };
  const scene = {
    GetCount: () => sceneData().cutsMs.length,
    GetAt: (index) =>
      Number.isInteger(index)
        ? (sceneData().cutsMs[index] ?? -1)
        : fail('Invalid scene index'),
    GetNext: (time) => sceneData().cutsMs[sceneIndex(time, false)] ?? -1,
    GetPrevious: (time) => sceneData().cutsMs[sceneIndex(time, true)] ?? -1,
    Contains: (start, stop) => {
      if (!Number.isSafeInteger(stop))
        fail('Scene lookup requires integer milliseconds');
      const next = scene.GetNext(start);
      return next !== -1 && next <= stop;
    },
    get StartOffset() {
      return sceneData().startOffsetMs;
    },
    get StopOffset() {
      return sceneData().stopOffsetMs;
    },
    get FilterOffset() {
      return sceneData().filterOffsetMs;
    },
  };
  const prefs = input.context.preferences;
  const core = {
    GetSubCount: () => state.length,
    GetSubAt: (index) =>
      Number.isInteger(index) ? legacy(index) : fail('Invalid subtitle index'),
    GetFirst: () => legacy(0),
    GetNext: (cue) => neighbor(cue, 1),
    GetPrevious: (cue) => neighbor(cue, -1),
    GetSelectedCount: () => selected.size,
    GetFirstSelected: () =>
      legacy(state.findIndex((cue) => selected.has(cue.id))),
    GetNextSelected: (cue) => {
      if (cue === null) return null;
      if (!identity.has(cue)) fail('Expected a captured VSS subtitle wrapper');
      for (let i = identity.get(cue) + 1; i < state.length; ++i)
        if (selected.has(state[i].id)) return legacy(i);
      return null;
    },
    GetAudioCursorPosition: () =>
      input.context.cursorMs ?? fail('Audio cursor is unavailable'),
    MinimumBlank: prefs.minimumBlankMs,
    MinimumDuration: prefs.minimumDurationMs,
    MaximumDuration: prefs.maximumDurationMs,
    CpsTarget: prefs.cpsTarget,
    get RsTargetIndex() {
      return prefs.rsTargetIndex ?? fail('RS target index is unavailable');
    },
  };
  expose('VSSCore', supported('VSSCore', core));
  expose('SceneChange', supported('SceneChange', scene));
  const log = (message) => {
    checkedString(message, 1024, 'Log message');
    logBytes += message.length * 4;
    if (logs.length >= 100 || logBytes > 16384)
      fail('Plugin log limit exceeded');
    logs.push(message);
  };
  expose('ScriptLog', log);
  let stack = [];
  const load = (name) => {
    checkedString(name, 255, 'Script path');
    name = name.replace(/\\/g, '/');
    if (name.startsWith('/') || /[:*?\0]/.test(name))
      fail('Only explicit package-local script paths are supported');
    const base = (stack.at(-1) ?? input.package.manifest.entry)
      .split('/')
      .slice(0, -1);
    for (const part of name.split('/')) {
      if (!part || part === '.') continue;
      if (part === '..') {
        if (!base.length) fail('Script path escapes the package');
        base.pop();
      } else base.push(part);
    }
    const path = base.join('/');
    if (!Object.hasOwn(input.package.files, path))
      fail(`Missing package script: ${path}`);
    if (stack.includes(path) || stack.length >= 16)
      fail('Cyclic or excessive package script loading');
    stack.push(path);
    try {
      evaluate(input.package.files[path]);
    } finally {
      stack.pop();
    }
    return true;
  };
  expose('LoadScript', load);
  const stringBytes = (value) => {
    let size = 0;
    for (const character of value) {
      const code = character.codePointAt(0);
      size += code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
      if (size > 4096) break;
    }
    return size;
  };
  const scalar = (value) =>
    typeof value === 'boolean' ||
    (typeof value === 'string' &&
      value.length <= 4096 &&
      stringBytes(value) <= 4096) ||
    (typeof value === 'number' &&
      Number.isFinite(value) &&
      Math.abs(value) <= Number.MAX_SAFE_INTEGER);
  let settings = { ...input.package.manifest.settings };
  const modern = (index) =>
    index < 0 || index >= state.length
      ? null
      : Object.freeze({ ...state[index] });
  const vsf = {
    version: 1,
    sourceFormat: input.format,
    preferences: Object.freeze({ ...prefs }),
    cursorMs: input.context.cursorMs,
    get settings() {
      return Object.freeze({ ...settings });
    },
    getCue: (id) => modern(indexOf(id)),
    getNext: (id) => modern(indexOf(id) + 1),
    getPrevious: (id) => modern(indexOf(id) - 1),
    getAllCues: () => Object.freeze(state.map((_, index) => modern(index))),
    getSelection: () =>
      Object.freeze(
        state
          .filter((cue) => selected.has(cue.id))
          .map((cue) => modern(indexOf(cue.id))),
      ),
    getSceneCuts: () => Object.freeze([...sceneData().cutsMs]),
    update: (id, patch) => {
      const index = indexOf(id);
      if (!patch || typeof patch !== 'object' || Array.isArray(patch))
        fail('Cue update must be an object');
      for (const [field, value] of Object.entries(patch)) {
        if (!['text', 'startMs', 'endMs'].includes(field))
          fail(`Unsupported cue update field: ${field}`);
        set(index, field, value);
      }
    },
    log,
  };
  expose('VSF', Object.freeze(vsf));
  Object.defineProperty(globalThis, 'VSSPlugin', {
    value: undefined,
    writable: true,
  });
  Object.defineProperty(globalThis, 'VSFPlugin', {
    value: undefined,
    writable: true,
  });
  // Entry is already canonical and package-root relative; nested loads are relative.
  stack = [input.package.manifest.entry];
  evaluate(input.package.files[input.package.manifest.entry]);
  stack = [];
  const isLegacy = input.package.manifest.api === 'vss-1';
  const plugin = isLegacy ? globalThis.VSSPlugin : globalThis.VSFPlugin;
  if (!plugin || typeof plugin !== 'object')
    fail('Plugin did not register its rule object');
  const check = plugin[isLegacy ? 'HasError' : 'check'];
  const fix = plugin[isLegacy ? 'FixError' : 'fix'];
  if (typeof check !== 'function') fail('Plugin must provide a checker');
  if (input.mode === 'fix' && typeof fix !== 'function')
    fail('Plugin has no reversible fix callback');
  if (isLegacy) {
    settings = {};
    for (const key of Object.keys(plugin).filter((name) =>
      name.startsWith('Param'),
    )) {
      const value = plugin[key]?.Value;
      if (!scalar(value)) fail(`Unsupported VSS parameter: ${key}`);
      settings[key] = value;
    }
  }
  for (const overrides of [input.package.manifest.settings, input.settings]) {
    for (const [key, value] of Object.entries(overrides)) {
      if (
        !Object.hasOwn(settings, key) ||
        !scalar(value) ||
        typeof value !== typeof settings[key]
      )
        fail(`Unknown or mistyped plugin setting: ${key}`);
      settings[key] = value;
      if (isLegacy) plugin[key].Value = value;
    }
  }
  if (Object.keys(settings).length > 64)
    fail('Plugin has more than 64 parameters');
  const name = isLegacy ? plugin.Name : input.package.manifest.name;
  checkedString(name, 256, 'Plugin name');
  for (let index = 0; index < state.length; ++index) {
    if (!scope.has(state[index].id)) continue;
    writable = false;
    const args = isLegacy
      ? [legacy(index), legacy(index - 1), legacy(index + 1)]
      : [modern(index), vsf];
    const message = check.apply(plugin, args);
    if (message !== null && typeof message !== 'string')
      fail('Checker must return a string or null');
    if (message) {
      checkedString(message, 1024, 'Finding');
      findings.push({ cueId: state[index].id, message });
      if (input.mode === 'fix') {
        writable = true;
        const result = fix.apply(plugin, args);
        writable = false;
        if (result && typeof result.then === 'function')
          fail('Async fix callbacks are unsupported');
      }
    }
  }
  if (poison) throw new Error(poison);
  return {
    version: 1,
    key: input.key,
    fingerprint: input.fingerprint,
    name,
    canFix: typeof fix === 'function',
    settings,
    findings,
    logs,
    edits: state.flatMap((cue, index) =>
      cue.startMs !== original[index].startMs ||
      cue.endMs !== original[index].endMs ||
      cue.text !== original[index].text
        ? [
            {
              cueId: cue.id,
              startMs: cue.startMs,
              endMs: cue.endMs,
              text: cue.text,
            },
          ]
        : [],
    ),
  };
}
// Rust appends the invocation after this declaration.
void pluginAdapter;
