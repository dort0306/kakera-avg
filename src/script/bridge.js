// Engine bridge, not game content. Its private registries never enter a save file.
(() => {
  "use strict";
  const parse = JSON.parse;
  const stringify = JSON.stringify;
  const freeze = Object.freeze;
  // frame -> {frame, kind, options: Map, routes: [], location}
  const registry = new Map();
  // category -> target -> event -> {run, location}; structured keys, never text joins.
  const uiRegistry = new Map();
  // Independent author events; definitions never require a target control.
  const eventRegistry = new Map();
  // Load-time composition tags; each definition is expanded into host nodes once.
  const componentRegistry = new Map();
  let sealed = false;
  const toString = String;
  const host = "<engine-bridge>";
  const fail = (code, parameters = {}) => { throw new Error("KDIAG" + stringify({code, parameters})); };
  const name = value => typeof value === "string" && value.length > 0
    ? value : fail("host-bridge-0001", {});
  const callback = value => typeof value === "function"
    ? value : fail("host-bridge-0002", {});
  const LIFECYCLE = ["frame.enter:", "frame.exit:", "screen.open:", "screen.close:"];
  const reserved = value => value === "game.enter" || LIFECYCLE.some(prefix => value.startsWith(prefix));
  // Keep the author's call site; only this bridge file's own frames are dropped.
  function location() {
    const stack = new Error().stack;
    if (typeof stack !== "string" || stack.length === 0) return "";
    const lines = stack.split("\n").filter(line => !line.includes(host));
    return (lines.length > 0 ? lines.join("\n").trim() : stack.trim());
  }
  const target = value => typeof value === "function"
    ? {run: value, target: null}
    : typeof value === "string" && value.length > 0
      ? {run: () => value, target: value}
      : fail("host-bridge-0030", {});
  function route(value, kind, where) {
    if (sealed) fail("host-bridge-0003", {});
    name(value);
    const old = registry.get(value);
    if (old && (kind !== "choice" || old.kind !== "choice")) fail("host-bridge-0004", {"arg0":toString(value)});
    if (old) return old;
    const created = {frame: value, kind, options: new Map(), routes: [], location: where};
    registry.set(value, created);
    return created;
  }
  function bind(category, id, event, run) {
    if (sealed) fail("host-bridge-0006", {});
    let targets = uiRegistry.get(category);
    if (!targets) { targets = new Map(); uiRegistry.set(category, targets); }
    let events = targets.get(id);
    if (!events) { events = new Map(); targets.set(id, events); }
    const old = events.get(event);
    if (old) fail("host-bridge-0007", {
      "arg0": toString(category), "arg1": toString(id), "arg2": toString(event), "arg3": old.location
    });
    events.set(event, {run: callback(run), location: location()});
  }
  function frameSelector(value) {
    const frame = name(value);
    return freeze({
      advance(value) {
        const route_ = route(frame, "advance", location());
        if (route_.routes.length > 0) fail("host-bridge-0004", {"arg0":toString(frame)});
        const entry = target(value);
        route_.run = entry.run;
        route_.routes.push({option: null, target: entry.target, location: location()});
      },
      choice(option, value, when) {
        const route_ = route(frame, "choice", location());
        name(option);
        if (route_.options.has(option)) fail("host-bridge-0005", {"arg0":toString(frame),"arg1":toString(option)});
        const entry = target(value);
        if (when !== undefined) callback(when);
        route_.options.set(option, {
          run: entry.run, when: when === undefined ? undefined : when
        });
        route_.routes.push({option, target: entry.target, location: location()});
      },
      end() { route(frame, "end", location()); },
      enter(run) { bind("frame.enter", frame, "", run); },
      exit(run) { bind("frame.exit", frame, "", run); }
    });
  }
  function uiSelector(value) {
    const id = name(value);
    return freeze({
      bind(event, run) {
        const key = name(event);
        if (run === undefined) {
          if (id === "$game" || id === "$host" || reserved(key) || key === "screen.open" || key === "screen.close")
            fail("host-bridge-0032", {"arg0":toString(key)});
          bind("ui", id, key, () => {});
          return;
        }
        if (id === "$game") {
          if (key !== "game.enter") fail("host-bridge-0031", {"arg0":toString(key)});
          bind("game", id, key, run);
          return;
        }
        if (id === "$host" || (key !== "screen.open" && key !== "screen.close"))
          fail("host-bridge-0033", {});
        bind("ui", id, key, run);
      }
    });
  }
  function plainObject(value) {
    if (typeof value !== "object" || value === null || Array.isArray(value))
      fail("host-bridge-0038", {});
    const proto = Object.getPrototypeOf(value);
    if (proto !== Object.prototype && proto !== null) fail("host-bridge-0038", {});
    return value;
  }
  // Structural validation of one expansion result. Author child tokens are opaque
  // frozen objects supplied by the host, never author-invented descriptors.
  function expansionTree(root, tokens) {
    const used = [];
    let total = 0;
    function walk(value, depth) {
      if (depth > 8) fail("host-bridge-0039", {});
      const node = plainObject(value);
      const keys = Object.keys(node);
      for (const key of keys) {
        if (key !== "tag" && key !== "attrs" && key !== "children")
          fail("host-bridge-0040", {"arg0": toString(key)});
        const descriptor = Object.getOwnPropertyDescriptor(node, key);
        if (!descriptor || !("value" in descriptor))
          fail("host-bridge-0041", {"arg0": toString(key)});
      }
      const tag = node.tag;
      if (typeof tag !== "string" || tag.length === 0) fail("host-bridge-0042", {});
      if (tag === "$author") {
        if (depth === 1) fail("host-bridge-0043", {});
        if (node.attrs === undefined || node.children !== undefined) fail("host-bridge-0044", {});
        const attrs = plainObject(node.attrs);
        const names = Object.keys(attrs);
        if (names.length !== 1 || names[0] !== "index") fail("host-bridge-0044", {});
        const index = attrs.index;
        if (!Number.isSafeInteger(index) || index < 0 || index >= tokens.length)
          fail("host-bridge-0045", {});
        if (tokens[index] !== value) fail("host-bridge-0046", {});
        used.push(index);
        return {tag, attrs: {index}};
      }
      total += 1;
      if (total > 1024) fail("host-bridge-0047", {});
      const result = {tag};
      if (node.attrs !== undefined) {
        const attrs = plainObject(node.attrs);
        const names = Object.keys(attrs);
        if (names.length > 64) fail("host-bridge-0048", {});
        const copy = {};
        for (const attribute of names) {
          const descriptor = Object.getOwnPropertyDescriptor(attrs, attribute);
          if (!descriptor || !("value" in descriptor))
            fail("host-bridge-0041", {"arg0": toString(attribute)});
          const item = descriptor.value;
          if (typeof item === "string" || typeof item === "boolean") copy[attribute] = item;
          else if (typeof item === "number" && Number.isFinite(item)) copy[attribute] = item;
          else fail("host-bridge-0049", {"arg0": toString(attribute)});
        }
        result.attrs = copy;
      }
      if (node.children !== undefined) {
        if (!Array.isArray(node.children) || node.children.length > 256) fail("host-bridge-0050", {});
        result.children = node.children.map(child => walk(child, depth + 1));
      }
      return result;
    }
    const tree = walk(root, 1);
    if (tree.tag !== "panel" && tree.tag !== "nine-patch") fail("host-bridge-0051", {});
    if (!tree.attrs || tree.attrs.id !== "$root") fail("host-bridge-0052", {});
    for (let index = 0; index < used.length; index += 1)
      if (used[index] !== index) fail("host-bridge-0053", {});
    if (used.length !== tokens.length) fail("host-bridge-0054", {});
    return tree;
  }
  function componentManifest() {
    return stringify([...componentRegistry].map(([tag, definition]) => ({
      name: tag, props: definition.props, expand: typeof definition.expand === "function",
      location: definition.location
    })));
  }
  function componentExpand(request) {
    const definition = componentRegistry.get(request.tag);
    if (!definition) fail("host-bridge-0055", {"arg0": toString(request.tag)});
    const tokens = [];
    for (let index = 0; index < request.children; index += 1)
      tokens.push(freeze({tag: "$author", attrs: freeze({index})}));
    const props = plainObject(request.props);
    const result = definition.expand(freeze({props: freeze(props), children: freeze(tokens)}));
    if (result instanceof Promise) fail("host-bridge-0056", {});
    const json = stringify(expansionTree(result, tokens));
    if (json.length > 262144) fail("host-bridge-0057", {});
    return json;
  }
  Object.defineProperty(globalThis, "frame", { value: frameSelector, writable: false, configurable: false });
  Object.defineProperty(globalThis, "ui", { value: uiSelector, writable: false, configurable: false });
  Object.defineProperty(globalThis, "event", { value: freeze({
    register(id, run) {
      if (sealed) fail("host-bridge-0006", {});
      const key = name(id);
      if (reserved(key) || key === "screen.open" || key === "screen.close")
        fail("host-bridge-0032", {"arg0":toString(key)});
      const old = eventRegistry.get(key);
      if (old) fail("host-bridge-0034", {"arg0":key, "arg1":old.location});
      eventRegistry.set(key, {run: callback(run), location: location()});
    }
  }), writable: false, configurable: false });
  Object.defineProperty(globalThis, "component", { value: freeze({
    register(tagName, definition) {
      if (sealed) fail("host-bridge-0006", {});
      const tag = name(tagName);
      if (tag.length > 64 || !/^x-[a-z0-9]+(?:-[a-z0-9]+)*$/.test(tag))
        fail("host-bridge-0058", {"arg0": toString(tag)});
      const config = plainObject(definition);
      for (const key of Object.keys(config)) {
        if (key !== "props" && key !== "expand")
          fail("host-bridge-0059", {"arg0": toString(key)});
        const descriptor = Object.getOwnPropertyDescriptor(config, key);
        if (!descriptor || !("value" in descriptor))
          fail("host-bridge-0041", {"arg0": toString(key)});
      }
      if (typeof config.expand !== "function") fail("host-bridge-0060", {});
      const props = plainObject(config.props);
      if (componentRegistry.has(tag)) fail("host-bridge-0061", {"arg0": toString(tag)});
      const copy = {};
      for (const key of Object.keys(props)) copy[key] = props[key];
      componentRegistry.set(tag, freeze({
        props: freeze(copy), expand: config.expand, location: location()
      }));
    }
  }), writable: false, configurable: false });
  // No host IO or module loader is installed. Remove nondeterministic intrinsics.
  for (const name of ["Date", "WeakRef", "FinalizationRegistry", "SharedArrayBuffer", "Atomics"])
    delete globalThis[name];
  Object.defineProperty(Math, "random", { value: () => fail("host-bridge-0008", {}), writable: false, configurable: false });

  function context(request, writable) {
    const draft = Object.create(null);
    for (const [name, item] of Object.entries(request.variables))
      draft[name] = item.type === "int" ? BigInt(item.value) : item.value;
    const effects = [];
    const integer = value => typeof value === "bigint" ? value
      : typeof value === "number" && Number.isSafeInteger(value) ? BigInt(value)
      : fail("host-bridge-0009", {});
    function set(name, value, add) {
      if (!writable) fail("host-bridge-0010", {});
      if (!Object.hasOwn(draft, name)) fail("host-bridge-0011", {"arg0":toString(name)});
      if (effects.length >= 128) fail("host-bridge-0012", {});
      if (typeof draft[name] === "bigint") value = integer(value);
      if (typeof value !== typeof draft[name]) fail("host-bridge-0013", {"arg0":toString(name)});
      if (add && typeof value !== "bigint") fail("host-bridge-0014", {});
      effects.push(add ? { op: "add", variable: name, amount: value.toString() }
        : { op: "set", variable: name, type: request.variables[name].type,
            value: typeof value === "bigint" ? value.toString() : value });
      draft[name] = add ? draft[name] + value : value;
    }
    return { effects, ctx: freeze({
      frameId: request.frameName ?? request.frame, optionId: request.choice ?? null,
      get vars() {
        return new Proxy(freeze({ ...draft }), {
          get(target, name) {
            if (typeof name === "string" && !Object.hasOwn(target, name))
              fail("host-bridge-0015", {"arg0":toString(name)});
            return Reflect.get(target, name);
          }
        });
      },
      set(name, value) { set(name, value, false); },
      add(name, value) { set(name, value, true); }
    }) };
  }
  function allowed(handler, request) {
    if (!handler.when) return true;
    const result = handler.when(context(request, false).ctx);
    if (typeof result !== "boolean") fail("host-bridge-0016", {});
    return result;
  }
  function lookup(category, target, event) {
    const targets = uiRegistry.get(category);
    const events = targets && targets.get(target);
    return events ? events.get(event) : undefined;
  }
  function uiEvent(request) {
    const ordinary = request.category === "host" || (request.category === "ui" && request.event !== "screen.open" && request.event !== "screen.close");
    const handler = ordinary ? eventRegistry.get(request.event) : lookup(request.category, request.target, request.event);
    if (!handler) fail("host-bridge-0017", {
      "arg0": toString(request.category) + "/" + toString(request.target) + "/" + toString(request.event)
    });
    const commands = [];
    const emit = command => { if (commands.length >= 32) fail("host-bridge-0018", {}); commands.push(command); };
    function readonly(value) {
      if (value && typeof value === "object") { for (const child of Object.values(value)) readonly(child); freeze(value); }
      return value;
    }
    const snapshot = readonly(request.snapshot);
    const ctx = freeze({
      game: snapshot.game, settings: snapshot.settings, saves: snapshot.saves,
      repeats: snapshot.repeat ?? freeze({}),
      localeInfo: snapshot.localeInfo,
      locale(channel, locale) { emit({type:"locale", channel, locale}); },
      shortcuts: snapshot.shortcuts, colors: snapshot.colors, vars: context(request, false).ctx.vars,
      ui: context({ variables: request.uiVariables }, false).ctx.vars,
      item: readonly(request.item), value: readonly(request.value),
      present(screen) { emit({type:"present",screen}); },
      resume(frame) { emit({type:"resume",frame:frame === undefined ? null : frame}); },
      playSound(id) { emit({type:"play_sound",id}); },
      sequence(id) { emit({type:"sequence",id}); },
      soundEnabled(enabled) { emit({type:"sound_enabled",enabled}); },
      soundActor(actor, enabled) { emit({type:"sound_actor",actor,enabled}); },
      soundExclude(asset, excluded) { emit({type:"sound_exclude",asset,excluded}); },
      open(screen) { emit({ type: "open", screen }); },
      close() { emit({ type: "close" }); },
      setUi(name, value) {
        let type = typeof value;
        if (type === "number") { if (!Number.isSafeInteger(value)) fail("host-bridge-0019", {}); value = BigInt(value); type = "bigint"; }
        if (type === "bigint") emit({ type: "set_ui", name, value: { type: "int", value: value.toString() } });
        else if (type === "boolean" || type === "string") emit({ type: "set_ui", name, value: { type: type === "boolean" ? "bool" : "string", value } });
        else fail("host-bridge-0020", {});
      },
      save(slot, overwrite = false) { emit({ type: "save", slot, overwrite }); },
      load(slot) { emit({ type: "load", slot }); },
      quit() { emit({ type: "quit" }); },
      captureShortcut(action, resultEvent) {
        emit({ type: "capture_shortcut", action, result_event: resultEvent });
      },
      cancelShortcutCapture() { emit({ type: "cancel_shortcut_capture" }); },
      keybinding(action, binding) {
        if (binding !== null && typeof binding !== "string") fail("host-bridge-0021", {});
        emit({ type: "keybinding", action, binding });
      },
      resetKeybindings() { emit({ type: "reset_keybindings" }); },
      colorPreference(slot, rgbaOrNull) {
        if (rgbaOrNull !== null && typeof rgbaOrNull !== "string") fail("host-bridge-0022", {});
        emit({ type: "color_preference", slot, value: rgbaOrNull });
      },
      resetColors() { emit({ type: "reset_colors" }); },
      restart(frame) { emit({ type: "restart", frame: frame === undefined ? null : String(frame) }); },
      advance() { emit({ type: "advance" }); },
      choose(option) { emit({ type: "choose", option }); },
      setting(name, value) { emit({ type: "setting", name, value }); },
      unlock(id) { emit({ type: "unlock", id }); },
      message(text) { emit({ type: "message", text }); },
      transition(kind, duration = 600, easing = "ease-in-out") { emit({ type: "transition", transition: { kind, duration, easing } }); },
      slotsPage(page) { emit({type:"slots_page",page}); },
      loadAuto(slot) { emit({type:"load_auto",slot}); },
      autoPlay(enabled) { emit({type:"auto_play",enabled}); },
      live2d(slot, motion) { emit({type:"live2d",slot,motion}); },
      portrait(slot, kind, duration = 600, strength = 32, easing = "ease-in-out") { emit({ type: "portrait", slot, animation: { kind, duration, strength, easing } }); },
      animate(target, kind, duration = 400, strength = 12, easing = "ease-in-out") { emit({ type: "animate", target, animation: { kind, duration, strength, easing } }); }
    });
    const result = handler.run(ctx);
    if (result !== undefined) fail("host-bridge-0023", {});
    return stringify(commands);
  }
  function uiManifest() {
    const out = [];
    for (const [event, handler] of eventRegistry)
      out.push({category: "registered", target: "", event, location: handler.location});
    for (const [category, targets] of uiRegistry)
      for (const [target, events] of targets)
        for (const [event, handler] of events)
          out.push({category, target, event, location: handler.location});
    return out;
  }
  return requestJson => {
    sealed = true;
    const request = parse(requestJson);
    if (request.mode === "ui-manifest") return stringify(uiManifest());
    if (request.mode === "ui") return uiEvent(request);
    if (request.mode === "component-manifest") return componentManifest();
    if (request.mode === "component-expand") return componentExpand(request);
    if (request.mode === "manifest") return stringify([...registry.values()].map(route => ({
      frame: route.frame, kind: route.kind, options: [...route.options.keys()],
      location: route.location, routes: route.routes
    })));
    const route = registry.get(request.frame);
    if (!route) fail("host-bridge-0024", {"arg0":toString(request.frame)});
    if (request.mode === "guard") {
      const handler = route.options.get(request.choice);
      if (!handler) fail("host-bridge-0025", {"arg0":toString(request.choice)});
      return stringify(allowed(handler, request));
    }
    if (request.choice == null && route.kind !== "advance") return "null";
    let handler = route;
    if (request.choice != null) {
      if (route.kind !== "choice") fail("host-bridge-0026", {});
      handler = route.options.get(request.choice);
      if (!handler) fail("host-bridge-0027", {"arg0":toString(request.choice)});
      if (!allowed(handler, request)) fail("host-bridge-0028", {"arg0":toString(request.choice)});
    }
    const { ctx, effects } = context(request, true);
    const to = handler.run(ctx);
    if (typeof to !== "string" || !to.length) fail("host-bridge-0029", {});
    return stringify({ to, effects });
  };
})()
