// Engine bridge, not game content. Its private registry never enters a save file.
(() => {
  "use strict";
  const parse = JSON.parse;
  const stringify = JSON.stringify;
  const freeze = Object.freeze;
  const registry = new Map();
  const uiRegistry = new Map();
  let sealed = false;
  const fail = message => { throw new Error(message); };
  const id = value => typeof value === "string" && value.length > 0
    ? value : fail("触发器 ID 必须为非空字符串");
  const callback = value => typeof value === "function"
    ? value : fail("回调必须为函数");
  function register(frame, kind) {
    if (sealed) fail("回调执行期间不能注册触发器");
    id(frame);
    const old = registry.get(frame);
    if (old && (kind !== "choice" || old.kind !== "choice")) fail(`重复场景触发器：${frame}`);
    if (old) return old;
    const route = { frame, kind, options: new Map(), location: new Error().stack };
    registry.set(frame, route);
    return route;
  }
  Object.defineProperty(globalThis, "on", { value: freeze({
    advance(frame, run) { const route = register(frame, "advance"); route.run = callback(run); },
    choice(frame, option, run, when) {
      const route = register(frame, "choice");
      id(option);
      if (route.options.has(option)) fail(`重复选项触发器：${frame}/${option}`);
      route.options.set(option, { run: callback(run), when: when === undefined ? undefined : callback(when) });
    },
    end(frame) { register(frame, "end"); },
    ui(event, run) {
      if (sealed) fail("回调执行期间不能注册 UI 事件");
      id(event);
      if (uiRegistry.has(event)) fail(`重复 UI 事件：${event}`);
      uiRegistry.set(event, callback(run));
    }
  }), writable: false, configurable: false });
  // No host IO or module loader is installed. Remove nondeterministic intrinsics.
  for (const name of ["Date", "WeakRef", "FinalizationRegistry", "SharedArrayBuffer", "Atomics"])
    delete globalThis[name];
  Object.defineProperty(Math, "random", { value: () => fail("随机不可用于可重放逻辑"), writable: false, configurable: false });

  function context(request, writable) {
    const draft = Object.create(null);
    for (const [name, item] of Object.entries(request.variables))
      draft[name] = item.type === "int" ? BigInt(item.value) : item.value;
    const effects = [];
    const integer = value => typeof value === "bigint" ? value
      : typeof value === "number" && Number.isSafeInteger(value) ? BigInt(value)
      : fail("整数更新需要 BigInt 或安全整数 Number");
    function set(name, value, add) {
      if (!writable) fail("条件回调不能更新变量");
      if (!Object.hasOwn(draft, name)) fail(`未知变量：${name}`);
      if (effects.length >= 128) fail("更新列表超过 128 项");
      if (typeof draft[name] === "bigint") value = integer(value);
      if (typeof value !== typeof draft[name]) fail(`变量类型不匹配：${name}`);
      if (add && typeof value !== "bigint") fail("add 仅支持 int");
      effects.push(add ? { op: "add", variable: name, amount: value.toString() }
        : { op: "set", variable: name, type: request.variables[name].type,
            value: typeof value === "bigint" ? value.toString() : value });
      draft[name] = add ? draft[name] + value : value;
    }
    return { effects, ctx: freeze({
      frameId: request.frame, optionId: request.choice ?? null,
      get vars() {
        return new Proxy(freeze({ ...draft }), {
          get(target, name) {
            if (typeof name === "string" && !Object.hasOwn(target, name))
              fail(`未知变量：${name}`);
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
    if (typeof result !== "boolean") fail("条件回调必须同步返回 bool");
    return result;
  }
  function uiEvent(request) {
    const handler = uiRegistry.get(request.event);
    if (!handler) fail(`未注册 UI 事件：${request.event}`);
    const commands = [];
    const emit = command => { if (commands.length >= 32) fail("UI 命令超过 32 条"); commands.push(command); };
    function readonly(value) {
      if (value && typeof value === "object") { for (const child of Object.values(value)) readonly(child); freeze(value); }
      return value;
    }
    const snapshot = readonly(request.snapshot);
    const ctx = freeze({
      game: snapshot.game, settings: snapshot.settings, saves: snapshot.saves,
      vars: context(request, false).ctx.vars,
      ui: context({ variables: request.uiVariables }, false).ctx.vars,
      item: readonly(request.item), value: readonly(request.value),
      open(screen) { emit({ type: "open", screen }); },
      close() { emit({ type: "close" }); },
      setUi(name, value) {
        let type = typeof value;
        if (type === "number") { if (!Number.isSafeInteger(value)) fail("UI 整数需要安全整数或 BigInt"); value = BigInt(value); type = "bigint"; }
        if (type === "bigint") emit({ type: "set_ui", name, value: { type: "int", value: value.toString() } });
        else if (type === "boolean" || type === "string") emit({ type: "set_ui", name, value: { type: type === "boolean" ? "bool" : "string", value } });
        else fail("UI 变量仅支持 int/bool/string");
      },
      save(slot, overwrite = false) { emit({ type: "save", slot, overwrite }); },
      load(slot) { emit({ type: "load", slot }); },
      importLegacy() { emit({ type: "import_legacy" }); },
      restart() { emit({ type: "restart" }); },
      advance() { emit({ type: "advance" }); },
      choose(option) { emit({ type: "choose", option }); },
      setting(name, value) { emit({ type: "setting", name, value }); },
      unlock(id) { emit({ type: "unlock", id }); },
      message(text) { emit({ type: "message", text }); },
      transition(kind, duration = 600, easing = "ease-in-out") { emit({ type: "transition", transition: { kind, duration, easing } }); },
      slotsPage(page) { emit({type:"slots_page",page}); },
      loadAuto(slot) { emit({type:"load_auto",slot}); },
      autoPlay(enabled) { emit({type:"auto_play",enabled}); },
      skipVideo() { emit({type:"skip_video"}); },
      live2d(slot, motion) { emit({type:"live2d",slot,motion}); },
      portrait(slot, kind, duration = 600, strength = 32, easing = "ease-in-out") { emit({ type: "portrait", slot, animation: { kind, duration, strength, easing } }); },
      animate(target, kind, duration = 400, strength = 12, easing = "ease-in-out") { emit({ type: "animate", target, animation: { kind, duration, strength, easing } }); }
    });
    const result = handler(ctx);
    if (result !== undefined) fail("UI 回调必须同步返回 undefined");
    return stringify(commands);
  }
  return requestJson => {
    sealed = true;
    const request = parse(requestJson);
    if (request.mode === "ui-manifest") return stringify([...uiRegistry.keys()]);
    if (request.mode === "ui") return uiEvent(request);
    if (request.mode === "manifest") return stringify([...registry.values()].map(route => ({
      frame: route.frame, kind: route.kind, options: [...route.options.keys()], location: route.location
    })));
    const route = registry.get(request.frame);
    if (!route) fail(`未注册场景触发器：${request.frame}`);
    if (request.mode === "guard") {
      const handler = route.options.get(request.choice);
      if (!handler) fail(`无效选项：${request.choice}`);
      return stringify(allowed(handler, request));
    }
    if (request.choice == null && route.kind !== "advance") return "null";
    let handler = route;
    if (request.choice != null) {
      if (route.kind !== "choice") fail("当前场景不能选择选项");
      handler = route.options.get(request.choice);
      if (!handler) fail(`无效选项：${request.choice}`);
      if (!allowed(handler, request)) fail(`选项条件不满足：${request.choice}`);
    }
    const { ctx, effects } = context(request, true);
    const to = handler.run(ctx);
    if (typeof to !== "string" || !to.length) fail("回调必须同步返回目标场景 ID");
    return stringify({ to, effects });
  };
})()
