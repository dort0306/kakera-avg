"use strict";

// Events exist independently of controls. XML references or ui(id).bind(eventId) attach them.
function closePage(ctx) { ctx.close(); }
event.register("close", closePage);

event.register("open-save", ctx => ctx.open("save"));
event.register("open-load", ctx => ctx.open("load"));
event.register("open-history", ctx => ctx.open("history"));
event.register("open-gallery", ctx => ctx.open("gallery"));

event.register("advance", ctx => ctx.advance());
ui("continue").bind("advance");
function chooseOption(ctx) {
  if (ctx.item.id === "portrait-demo") ctx.open("portrait-stage");
  if (ctx.item.id === "return" && ctx.game.frameId.startsWith("actor.")) ctx.open("hud");
  if (ctx.game.frameId.startsWith("live2d.") && ctx.item.id === "tap") {
    ctx.live2d("center", "TapBody:0");
  }
  ctx.choose(ctx.item.id);
}
event.register("choose", chooseOption);

function askConfirmation(ctx, action, title, message, acceptText) {
  ctx.setUi("dialogAction", action);
  ctx.setUi("dialogTitle", title);
  ctx.setUi("dialogHasTitle", title.length > 0);
  ctx.setUi("dialogMessage", message);
  ctx.setUi("dialogAcceptText", acceptText);
  ctx.setUi("dialogCancelText", "取消");
  ctx.open("confirmation");
}
event.register("restart", ctx => askConfirmation(ctx, "restart", "", "返回主菜单并重新选择故事起点？未保存的剧情进度不会保留。", "返回主菜单"));
{
  event.register("request-quit", ctx => askConfirmation(ctx, "quit", "退出游戏", "确定退出游戏？未保存的剧情进度不会保留。", "退出游戏"));
}
event.register("dialog-accept", ctx => {
  if (ctx.ui.dialogAction === "quit") {
    ctx.quit();
  } else if (ctx.ui.dialogAction === "restart") {
    ctx.close();
    ctx.open("title");
  } else {
    throw new Error("未知确认操作");
  }
});
event.register("start-main", ctx => { ctx.open("hud"); ctx.restart("opening"); });
event.register("start-other", ctx => { ctx.open("hud"); ctx.restart("alternate.opening"); });
frame("alternate.opening").end();

event.register("save-slot", ctx => {
  if (ctx.item.exists) {
    ctx.setUi("pendingSlot", ctx.item.slot);
    ctx.setUi("confirmText", `确认覆盖槽位 ${ctx.item.slot}？`);
    ctx.open("confirm-save");
  } else {
    ctx.save(ctx.item.slot);
    ctx.setUi("status", "存档完成");
    ctx.close();
  }
});
event.register("confirm-save", ctx => {
  ctx.save(Number(ctx.ui.pendingSlot), true);
  ctx.setUi("status", "覆盖完成");
  ctx.close();
  ctx.close();
});
event.register("load-slot", ctx => {
  ctx.load(ctx.item.slot);
  ctx.setUi("status", "读档完成");
  ctx.close();
  ctx.open("hud");
});

event.register("text-speed", ctx => ctx.setting("text_speed", ctx.value));
event.register("mute", ctx => ctx.setting("muted", ctx.value));
event.register("bgm-volume", ctx => ctx.setting("bgm_volume", ctx.value));
event.register("voice-volume", ctx => ctx.setting("voice_volume", ctx.value));
event.register("se-volume", ctx => ctx.setting("se_volume", ctx.value));

// Shortcut drafts live in UI strings. Only "应用" writes keybindings.json.
const shortcutDrafts = {advance: "shortcutAdvanceDraft", fast_forward: "shortcutFastForwardDraft", skip_sequence: "shortcutSkipDraft"};
const shortcutDefaults = {advance: "Space", fast_forward: "Ctrl", skip_sequence: "Space"};
function beginShortcutCapture(ctx, action) {
  ctx.captureShortcut(action, "shortcut-captured");
  ctx.setUi("shortcutStatus", "请按下新按键；Escape 取消捕获。");
}
event.register("shortcut-capture-advance", ctx => beginShortcutCapture(ctx, "advance"));
event.register("shortcut-capture-fast", ctx => beginShortcutCapture(ctx, "fast_forward"));
event.register("shortcut-capture-skip", ctx => beginShortcutCapture(ctx, "skip_sequence"));
// The capture result goes to the fixed $host target, never to an arbitrary control.
event.register("shortcut-captured", ctx => {
  const draft = shortcutDrafts[ctx.item.action];
  if (!draft) return;
  ctx.setUi(draft, ctx.value);
  ctx.setUi("shortcutStatus", `已捕获 ${ctx.value}；点“应用”保存，或继续更改。`);
});
event.register("shortcut-clear-advance", ctx => {
  ctx.setUi("shortcutAdvanceDraft", "");
  ctx.setUi("shortcutStatus", "草稿已清空；点“应用”后该键盘动作禁用。");
});
event.register("shortcut-clear-fast", ctx => {
  ctx.setUi("shortcutFastForwardDraft", "");
  ctx.setUi("shortcutStatus", "草稿已清空；点“应用”后该键盘动作禁用。");
});
event.register("shortcut-clear-skip", ctx => {
  ctx.setUi("shortcutSkipDraft", "");
  ctx.setUi("shortcutStatus", "草稿已清空；点“应用”后该键盘动作禁用。");
});
event.register("shortcut-apply", ctx => {
  for (const [action, draft] of Object.entries(shortcutDrafts)) {
    const text = ctx.ui[draft];
    ctx.keybinding(action, text.length === 0 ? null : text);
  }
  ctx.setUi("shortcutStatus", "已应用快捷键并写入独立偏好文件。");
});
event.register("shortcut-reset", ctx => {
  ctx.resetKeybindings();
  for (const [action, draft] of Object.entries(shortcutDrafts)) ctx.setUi(draft, shortcutDefaults[action]);
  ctx.setUi("shortcutStatus", "已恢复默认键位。");
});
event.register("shortcut-cancel", ctx => {
  ctx.cancelShortcutCapture();
  ctx.setUi("shortcutStatus", "已取消快捷键修改。");
  ctx.close();
});
ui("shortcuts").bind("screen.open", ctx => {
  for (const action of ["advance", "fast_forward", "skip_sequence"]) {
    const binding = ctx.shortcuts.bindings[action];
    ctx.setUi(shortcutDrafts[action], binding === null ? "" : binding);
  }
  ctx.setUi("shortcutStatus", "");
});

// Player palette: drafts stay in ordinary UI strings; only "应用" persists overrides.
const colorDrafts = {dialogue_background: "dialogueBgDraft", dialogue_text: "dialogueTextDraft"};
const colorDefaults = {dialogue_background: "#10253BFF", dialogue_text: "#F4EAD8FF"};
event.register("color-bg", ctx => ctx.setUi("dialogueBgDraft", ctx.value));
event.register("color-text", ctx => ctx.setUi("dialogueTextDraft", ctx.value));
event.register("color-apply", ctx => {
  for (const slot of ["dialogue_background", "dialogue_text"]) {
    ctx.colorPreference(slot, ctx.ui[colorDrafts[slot]]);
  }
  ctx.setUi("colorStatus", "已应用并写入配色偏好。");
});
event.register("color-reset", ctx => {
  ctx.resetColors();
  for (const slot of ["dialogue_background", "dialogue_text"]) ctx.setUi(colorDrafts[slot], colorDefaults[slot]);
  ctx.setUi("colorStatus", "已恢复作者默认配色。");
});
event.register("color-cancel", ctx => {
  ctx.setUi("colorStatus", "已取消本次配色修改。");
  ctx.close();
});
ui("colors").bind("screen.open", ctx => {
  for (const slot of ["dialogue_background", "dialogue_text"]) {
    ctx.setUi(colorDrafts[slot], ctx.colors[slot]);
  }
  ctx.setUi("colorStatus", "");
});

// One viewer page and one image node. The stable table below is the author's
// browsable collection; unknown unlocked gallery items open as a single image.
const cgBrowse = [
  {id: "cg.room", image: "bg.room", title: "午后房间"},
  {id: "cg.portrait", image: "portrait.lin", title: "凛的立绘"},
  {id: "cg.square", image: "cg.square", title: "透明方图"},
];
function cgPosition(id) { return cgBrowse.findIndex(entry => entry.id === id); }
function showCg(ctx, entry, position, toolbar) {
  ctx.setUi("cgImage", entry.image);
  ctx.setUi("cgTitle", entry.title);
  ctx.setUi("cgCurrent", entry.id);
  ctx.setUi("cgToolbar", toolbar);
  ctx.setUi("cgCounter", position >= 0 ? `${position + 1} / ${cgBrowse.length}` : "");
  ctx.setUi("cgPrevOk", position > 0);
  ctx.setUi("cgNextOk", position >= 0 && position < cgBrowse.length - 1);
}
event.register("view-cg", ctx => {
  if (!ctx.item.unlocked) return;
  const position = cgPosition(ctx.item.id);
  if (position >= 0) {
    showCg(ctx, cgBrowse[position], position, true);
  } else {
    showCg(ctx, {id: ctx.item.id, image: ctx.item.image, title: ctx.item.title}, -1, true);
  }
  // Opening the page is a separate command; switching inside the page only updates values.
  ctx.open("cg-view");
});
event.register("cg-prev", ctx => {
  const position = cgPosition(ctx.ui.cgCurrent);
  if (position <= 0) return;
  showCg(ctx, cgBrowse[position - 1], position - 1, ctx.ui.cgToolbar);
});
event.register("cg-next", ctx => {
  const position = cgPosition(ctx.ui.cgCurrent);
  if (position < 0 || position + 1 >= cgBrowse.length) return;
  showCg(ctx, cgBrowse[position + 1], position + 1, ctx.ui.cgToolbar);
});
event.register("cg-hide-toolbar", ctx => ctx.setUi("cgToolbar", false));
event.register("cg-background", ctx => {
  // The first background click after hiding only restores the toolbar.
  if (!ctx.ui.cgToolbar) { ctx.setUi("cgToolbar", true); return; }
  ctx.close();
});
ui("$game").bind("game.enter", ctx => {
  if (ctx.game.cgId) ctx.unlock(ctx.game.cgId);
  ctx.setUi("windowTitle", `午后物语 · ${ctx.game.frameId}`);
  if (ctx.ui.fxTransition) ctx.transition(ctx.ui.fxTransition, Number(ctx.ui.fxDuration));
  ctx.setUi("portraitMode", ctx.game.frameId.startsWith("actor."));
  if (ctx.game.frameId === "accept") ctx.unlock("cg.lin");
});

event.register("open-effects", ctx => ctx.open("effects"));
event.register("fx-duration", ctx => ctx.setUi("fxDuration", Math.round(ctx.value)));
event.register("fx-pause", ctx => {ctx.setUi("fxTransition","fade-white");ctx.setUi("fxDuration",5000);ctx.close();ctx.open("history");ctx.restart("opening");});
event.register("fx-skip-paused", ctx => {ctx.close();ctx.advance();});
const animatedEffects = [
  ["fxFade", "fx-fade", "fade-in"], ["fxZoom", "fx-zoom", "zoom-in"],
  ["fxShake", "fx-shake", "shake"], ["fxFlash", "fx-flash", "flash"],
  ["fxSlide", "fx-slide", "slide-left"],
];
for (const [target, eventId, kind] of animatedEffects) {
  event.register(eventId, ctx => ctx.animate("effectsPreview", kind, 1800, 36));
}
event.register("fx-scene", ctx => {ctx.close();ctx.animate("scene","shake",1800,24);});
const transitionEffects = [
  ["fxDissolve", "fx-dissolve", "dissolve"], ["fxBlack", "fx-black", "fade-black"],
  ["fxWhite", "fx-white", "fade-white"], ["fxDefault", "fx-default", ""],
  ["fxWipeL", "fx-wipe-left", "wipe-left"], ["fxWipeR", "fx-wipe-right", "wipe-right"],
  ["fxWipeU", "fx-wipe-up", "wipe-up"], ["fxWipeD", "fx-wipe-down", "wipe-down"],
  ["fxSlideL", "fx-slide-left", "slide-left"], ["fxSlideR", "fx-slide-right", "slide-right"],
  ["fxSlideU", "fx-slide-up", "slide-up"], ["fxSlideD", "fx-slide-down", "slide-down"],
];
for (const [target, eventId, kind] of transitionEffects) {
  event.register(eventId, ctx => {ctx.setUi("fxTransition",kind);ctx.close();ctx.restart("opening");});
}

event.register("open-portrait-stage", ctx => ctx.open("portrait-stage"));
event.register("portrait-hud", ctx => ctx.open("hud"));
const actorReplay = {"actor.stage": "shake", "actor.left": "enter-left", "actor.right": "enter-right", "actor.up": "enter-up", "actor.down": "enter-down", "actor.exit-left": "exit-left", "actor.exit-right": "exit-right", "actor.fade": "fade-in", "actor.shake": "shake", "actor.bounce": "bounce", "actor.turn": "turn", "actor.spin": "spin", "actor.fall-left": "fall-left", "actor.fall-right": "fall-right", "actor.fade-out": "fade-out", "actor.exit-up": "exit-up", "actor.exit-down": "exit-down"};
event.register("portrait-replay", ctx => {
  const action = actorReplay[ctx.game.frameId];
  if (action) ctx.portrait("center", action, 1800, 36);
});

event.register("toggle-auto", ctx => ctx.autoPlay(!ctx.game.autoPlaying));
event.register("slots-prev", ctx => ctx.slotsPage(ctx.saves.page - 1));
event.register("slots-next", ctx => ctx.slotsPage(ctx.saves.page + 1));
event.register("slots-last", ctx => ctx.slotsPage(20));
event.register("slots-page", ctx => ctx.slotsPage(Math.round(ctx.value)));
event.register("open-autosaves", ctx => ctx.open("autosaves"));
event.register("load-auto-slot", ctx => {ctx.loadAuto(ctx.item.slot);ctx.setUi("status","自动存档读取完成");ctx.close();ctx.close();ctx.open("hud");});
event.register("auto-delay", ctx => ctx.setting("auto_delay", ctx.value));
event.register("autosave-setting", ctx => ctx.setting("autosave", ctx.value));
event.register("skip-all", ctx => ctx.setting("skip_all", ctx.value));
event.register("video-volume", ctx => ctx.setting("video_volume", ctx.value));
event.register("live2d-tap", ctx => ctx.live2d("center", "TapBody:0"));

// Background input belongs to the real top screen, so both staged pages register.
function backgroundAdvance(ctx) {
  if (!ctx.ui.hudVisible) {ctx.setUi("hudVisible",true);return;}
  if (ctx.game.canAdvance) ctx.advance();
}
function toggleHud(ctx) { ctx.setUi("hudVisible",!ctx.ui.hudVisible); }
{
  event.register("background-advance", backgroundAdvance);
  event.register("toggle-hud", toggleHud);
}

event.register("fullscreen", ctx => ctx.setting("fullscreen", ctx.value));


event.register("open-skins", ctx => ctx.open("skins"));
event.register("window-title", ctx => ctx.setUi("windowTitle",
  ctx.ui.windowTitle === "自定义标题 · 中文 & JS" ? `午后物语 · ${ctx.game.frameId}` : "自定义标题 · 中文 & JS"));
const spriteRegions = [["spriteBlueButton", "sprite-blue", "0,0,64,64"], ["spriteGreenButton", "sprite-green", "64,0,64,64"], ["spriteRedButton", "sprite-red", "128,0,64,64"]];
for (const [target, eventId, region] of spriteRegions) {
  event.register(eventId, ctx => ctx.setUi("spriteRegion", region));
}

// Presentation is transient; preferences are independent of story slots.
event.register("ui-fade", ctx => ctx.sequence("ui.fade"));
event.register("ui-intro", ctx => ctx.sequence("ui.intro"));
event.register("ui-credits", ctx => ctx.sequence("ui.credits"));
event.register("ui-sound-enabled", ctx => ctx.soundEnabled(ctx.value));
event.register("ui-sound-lin", ctx => ctx.soundActor("lin", ctx.value));
event.register("ui-sound-exclude", ctx => ctx.soundExclude("voice.lin", ctx.value));

// Frame video tags were removed in v0.9.0: lifecycle callbacks hand off to UI.
frame("intro").exit(ctx => ctx.open("story-intro"));
ui("story-intro").bind("screen.open", ctx => ctx.sequence("story.intro"));
ui("story-intro").bind("screen.close", ctx => ctx.resume());
frame("credits").enter(ctx => ctx.open("story-credits"));
ui("story-credits").bind("screen.open", ctx => ctx.sequence("story.credits"));
ui("story-credits").bind("screen.close", ctx => ctx.resume("opening"));

// v0.11.2: author composition tags expand once into plain host nodes. The prefix
// comes from the instance id, so nested local ids stay predictable.
component.register("x-section", {
  props: {
    heading: { type: "string", required: true },
    open: { type: "binding", valueType: "bool", required: true }
  },
  expand({ props, children }) {
    return {
      tag: "panel",
      attrs: { id: "$root", color: "#0E1E30F2", visible: props.open },
      children: [
        { tag: "label", attrs: { id: "heading", x: 14, y: 10, width: 500, height: 30, text: props.heading, "font-size": 20, color: "#F0CA89" } },
        { tag: "panel", attrs: { id: "body", x: 12, y: 44, width: 536, height: 488 }, children }
      ]
    };
  }
});

// Generic repeat paging: each repeat has its own UI int and metadata snapshot.
event.register("demo-tab-gallery", ctx => { ctx.setUi("tabGallery", true); ctx.setUi("tabSlots", false); });
event.register("demo-tab-slots", ctx => { ctx.setUi("tabGallery", false); ctx.setUi("tabSlots", true); });
event.register("demo-gallery-prev", ctx => {
  const p = ctx.repeats["galleryTab.demoGallery"];
  if (p.canPrev) ctx.setUi("galleryPage", p.page - 1);
});
event.register("demo-gallery-next", ctx => {
  const p = ctx.repeats["galleryTab.demoGallery"];
  if (p.canNext) ctx.setUi("galleryPage", p.page + 1);
});
event.register("demo-slot-prev", ctx => {
  const p = ctx.repeats["slotsTab.demoSlots"];
  if (p.canPrev) ctx.setUi("slotPage", p.page - 1);
});
event.register("demo-slot-next", ctx => {
  const p = ctx.repeats["slotsTab.demoSlots"];
  if (p.canNext) ctx.setUi("slotPage", p.page + 1);
});
