"use strict";

// The same engine capabilities can be composed into entirely different XML pages.
on.ui("open-save", ctx => ctx.open("save"));
on.ui("open-load", ctx => ctx.open("load"));
on.ui("open-settings", ctx => ctx.open("settings"));
on.ui("open-history", ctx => ctx.open("history"));
on.ui("open-gallery", ctx => ctx.open("gallery"));
on.ui("close", ctx => ctx.close());
on.ui("advance", ctx => ctx.advance());
on.ui("choose", ctx => {
  if (ctx.item.id === "portrait-demo") ctx.open("portrait-stage");
  if (ctx.item.id === "return" && ctx.game.frameId.startsWith("actor.")) ctx.open("hud");
  if (ctx.game.frameId.startsWith("live2d.") && ctx.item.id === "tap") {
    ctx.live2d("center", "TapBody:0");
  }
  ctx.choose(ctx.item.id);
});
function askConfirmation(ctx, action, title, message, acceptText) {
  ctx.setUi("dialogAction", action);
  ctx.setUi("dialogTitle", title);
  ctx.setUi("dialogHasTitle", title.length > 0);
  ctx.setUi("dialogMessage", message);
  ctx.setUi("dialogAcceptText", acceptText);
  ctx.setUi("dialogCancelText", "取消");
  ctx.open("confirmation");
}
on.ui("restart", ctx => askConfirmation(ctx, "restart", "", "返回主菜单并重新选择故事起点？未保存的剧情进度不会保留。", "返回主菜单"));
on.ui("request-quit", ctx => askConfirmation(ctx, "quit", "退出游戏", "确定退出游戏？未保存的剧情进度不会保留。", "退出游戏"));
on.ui("dialog-accept", ctx => {
  if (ctx.ui.dialogAction === "quit") {
    ctx.quit();
  } else if (ctx.ui.dialogAction === "restart") {
    ctx.close();
    ctx.open("title");
  } else {
    throw new Error("未知确认操作");
  }
});
on.ui("start-main", ctx => { ctx.open("hud"); ctx.restart("opening"); });
on.ui("start-other", ctx => { ctx.open("hud"); ctx.restart("alternate.opening"); });
on.end("alternate.opening");

on.ui("save-slot", ctx => {
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
on.ui("confirm-save", ctx => {
  ctx.save(Number(ctx.ui.pendingSlot), true);
  ctx.setUi("status", "覆盖完成");
  ctx.close();
  ctx.close();
});
on.ui("load-slot", ctx => {
  ctx.load(ctx.item.slot);
  ctx.setUi("status", "读档完成");
  ctx.close();
  ctx.open("hud");
});

on.ui("text-speed", ctx => ctx.setting("text_speed", ctx.value));
on.ui("mute", ctx => ctx.setting("muted", ctx.value));
on.ui("bgm-volume", ctx => ctx.setting("bgm_volume", ctx.value));
on.ui("voice-volume", ctx => ctx.setting("voice_volume", ctx.value));
on.ui("se-volume", ctx => ctx.setting("se_volume", ctx.value));

// Shortcut drafts live in UI strings. Only "应用" writes keybindings.json.
const shortcutDrafts = {advance: "shortcutAdvanceDraft", fast_forward: "shortcutFastForwardDraft", skip_sequence: "shortcutSkipDraft"};
const shortcutDefaults = {advance: "Space", fast_forward: "Ctrl", skip_sequence: "Space"};
function beginShortcutCapture(ctx, action) {
  ctx.captureShortcut(action, "shortcut-captured");
  ctx.setUi("shortcutStatus", "请按下新按键；Escape 取消捕获。");
}
on.ui("shortcut-capture-advance", ctx => beginShortcutCapture(ctx, "advance"));
on.ui("shortcut-capture-fast", ctx => beginShortcutCapture(ctx, "fast_forward"));
on.ui("shortcut-capture-skip", ctx => beginShortcutCapture(ctx, "skip_sequence"));
on.ui("shortcut-captured", ctx => {
  const draft = shortcutDrafts[ctx.item.action];
  if (!draft) return;
  ctx.setUi(draft, ctx.value);
  ctx.setUi("shortcutStatus", `已捕获 ${ctx.value}；点“应用”保存，或继续更改。`);
});
on.ui("shortcut-clear-advance", ctx => {
  ctx.setUi("shortcutAdvanceDraft", "");
  ctx.setUi("shortcutStatus", "草稿已清空；点“应用”后该键盘动作禁用。");
});
on.ui("shortcut-clear-fast", ctx => {
  ctx.setUi("shortcutFastForwardDraft", "");
  ctx.setUi("shortcutStatus", "草稿已清空；点“应用”后该键盘动作禁用。");
});
on.ui("shortcut-clear-skip", ctx => {
  ctx.setUi("shortcutSkipDraft", "");
  ctx.setUi("shortcutStatus", "草稿已清空；点“应用”后该键盘动作禁用。");
});
on.ui("shortcut-apply", ctx => {
  for (const [action, draft] of Object.entries(shortcutDrafts)) {
    const text = ctx.ui[draft];
    ctx.keybinding(action, text.length === 0 ? null : text);
  }
  ctx.setUi("shortcutStatus", "已应用快捷键并写入独立偏好文件。");
});
on.ui("shortcut-reset", ctx => {
  ctx.resetKeybindings();
  for (const [action, draft] of Object.entries(shortcutDrafts)) ctx.setUi(draft, shortcutDefaults[action]);
  ctx.setUi("shortcutStatus", "已恢复默认键位。");
});
on.ui("shortcut-cancel", ctx => {
  ctx.cancelShortcutCapture();
  ctx.setUi("shortcutStatus", "已取消快捷键修改。");
  ctx.close();
});
on.screenOpen("shortcuts", ctx => {
  for (const action of ["advance", "fast_forward", "skip_sequence"]) {
    const binding = ctx.shortcuts.bindings[action];
    ctx.setUi(shortcutDrafts[action], binding === null ? "" : binding);
  }
  ctx.setUi("shortcutStatus", "");
});

// Player palette: drafts stay in ordinary UI strings; only "应用" persists overrides.
const colorDrafts = {dialogue_background: "dialogueBgDraft", dialogue_text: "dialogueTextDraft"};
const colorDefaults = {dialogue_background: "#10253BFF", dialogue_text: "#F4EAD8FF"};
on.ui("color-bg", ctx => ctx.setUi("dialogueBgDraft", ctx.value));
on.ui("color-text", ctx => ctx.setUi("dialogueTextDraft", ctx.value));
on.ui("color-apply", ctx => {
  for (const slot of ["dialogue_background", "dialogue_text"]) {
    ctx.colorPreference(slot, ctx.ui[colorDrafts[slot]]);
  }
  ctx.setUi("colorStatus", "已应用并写入配色偏好。");
});
on.ui("color-reset", ctx => {
  ctx.resetColors();
  for (const slot of ["dialogue_background", "dialogue_text"]) ctx.setUi(colorDrafts[slot], colorDefaults[slot]);
  ctx.setUi("colorStatus", "已恢复作者默认配色。");
});
on.ui("color-cancel", ctx => {
  ctx.setUi("colorStatus", "已取消本次配色修改。");
  ctx.close();
});
on.screenOpen("colors", ctx => {
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
on.ui("view-cg", ctx => {
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
on.ui("cg-prev", ctx => {
  const position = cgPosition(ctx.ui.cgCurrent);
  if (position <= 0) return;
  showCg(ctx, cgBrowse[position - 1], position - 1, ctx.ui.cgToolbar);
});
on.ui("cg-next", ctx => {
  const position = cgPosition(ctx.ui.cgCurrent);
  if (position < 0 || position + 1 >= cgBrowse.length) return;
  showCg(ctx, cgBrowse[position + 1], position + 1, ctx.ui.cgToolbar);
});
on.ui("cg-hide-toolbar", ctx => ctx.setUi("cgToolbar", false));
on.ui("cg-background", ctx => {
  // The first background click after hiding only restores the toolbar.
  if (!ctx.ui.cgToolbar) { ctx.setUi("cgToolbar", true); return; }
  ctx.close();
});
on.ui("game.enter", ctx => {
  if (ctx.game.cgId) ctx.unlock(ctx.game.cgId);
  ctx.setUi("windowTitle", `午后物语 · ${ctx.game.frameId}`);
  if (ctx.ui.fxTransition) ctx.transition(ctx.ui.fxTransition, Number(ctx.ui.fxDuration));
  ctx.setUi("portraitMode", ctx.game.frameId.startsWith("actor."));
  if (ctx.game.frameId === "accept") ctx.unlock("cg.lin");
});

on.ui("open-effects", ctx => ctx.open("effects"));
on.ui("fx-duration", ctx => ctx.setUi("fxDuration", Math.round(ctx.value)));
on.ui("fx-pause", ctx => {ctx.setUi("fxTransition","fade-white");ctx.setUi("fxDuration",5000);ctx.close();ctx.open("history");ctx.restart("opening");});
on.ui("fx-skip-paused", ctx => {ctx.close();ctx.advance();});
for (const [event, kind] of [["fx-fade","fade-in"],["fx-zoom","zoom-in"],["fx-shake","shake"],["fx-flash","flash"],["fx-slide","slide-left"]]) {
  on.ui(event, ctx => ctx.animate("effectsPreview", kind, 1800, 36));
}
on.ui("fx-scene", ctx => {ctx.close();ctx.animate("scene","shake",1800,24);});
for (const [event, kind] of [["fx-dissolve","dissolve"],["fx-black","fade-black"],["fx-white","fade-white"],
  ...["left","right","up","down"].map(d=>[`fx-wipe-${d}`,`wipe-${d}`]),
  ...["left","right","up","down"].map(d=>[`fx-slide-${d}`,`slide-${d}`]),["fx-default",""]]) {
  on.ui(event, ctx => {ctx.setUi("fxTransition",kind);ctx.close();ctx.restart("opening");});
}

on.ui("open-portrait-stage", ctx => ctx.open("portrait-stage"));
on.ui("portrait-hud", ctx => ctx.open("hud"));
const actorReplay = {"actor.stage": "shake", "actor.left": "enter-left", "actor.right": "enter-right", "actor.up": "enter-up", "actor.down": "enter-down", "actor.exit-left": "exit-left", "actor.exit-right": "exit-right", "actor.fade": "fade-in", "actor.shake": "shake", "actor.bounce": "bounce", "actor.turn": "turn", "actor.spin": "spin", "actor.fall-left": "fall-left", "actor.fall-right": "fall-right", "actor.fade-out": "fade-out", "actor.exit-up": "exit-up", "actor.exit-down": "exit-down"};
on.ui("portrait-replay", ctx => {
  const action = actorReplay[ctx.game.frameId];
  if (action) ctx.portrait("center", action, 1800, 36);
});

on.ui("toggle-auto", ctx => ctx.autoPlay(!ctx.game.autoPlaying));
on.ui("slots-prev", ctx => ctx.slotsPage(ctx.saves.page - 1));
on.ui("slots-next", ctx => ctx.slotsPage(ctx.saves.page + 1));
on.ui("slots-last", ctx => ctx.slotsPage(20));
on.ui("slots-page", ctx => ctx.slotsPage(Math.round(ctx.value)));
on.ui("open-autosaves", ctx => ctx.open("autosaves"));
on.ui("load-auto-slot", ctx => {ctx.loadAuto(ctx.item.slot);ctx.setUi("status","自动存档读取完成");ctx.close();ctx.close();ctx.open("hud");});
on.ui("auto-delay", ctx => ctx.setting("auto_delay", ctx.value));
on.ui("autosave-setting", ctx => ctx.setting("autosave", ctx.value));
on.ui("skip-all", ctx => ctx.setting("skip_all", ctx.value));
on.ui("video-volume", ctx => ctx.setting("video_volume", ctx.value));
on.ui("live2d-tap", ctx => ctx.live2d("center", "TapBody:0"));

on.ui("background-advance", ctx => {
  if (!ctx.ui.hudVisible) {ctx.setUi("hudVisible",true);return;}
  if (ctx.game.canAdvance) ctx.advance();
});
on.ui("toggle-hud", ctx => ctx.setUi("hudVisible",!ctx.ui.hudVisible));

on.ui("fullscreen", ctx => ctx.setting("fullscreen", ctx.value));


on.ui("open-skins", ctx => ctx.open("skins"));
on.ui("window-title", ctx => ctx.setUi("windowTitle",
  ctx.ui.windowTitle === "自定义标题 · 中文 & JS" ? `午后物语 · ${ctx.game.frameId}` : "自定义标题 · 中文 & JS"));
for (const [event, region] of [["sprite-blue","0,0,64,64"],["sprite-green","64,0,64,64"],["sprite-red","128,0,64,64"]]) {
  on.ui(event, ctx => ctx.setUi("spriteRegion", region));
}

// Presentation is transient; preferences are independent of story slots.
on.ui("ui-fade", ctx => ctx.sequence("ui.fade"));
on.ui("ui-intro", ctx => ctx.sequence("ui.intro"));
on.ui("ui-credits", ctx => ctx.sequence("ui.credits"));
on.ui("ui-sound-enabled", ctx => ctx.soundEnabled(ctx.value));
on.ui("ui-sound-lin", ctx => ctx.soundActor("lin", ctx.value));
on.ui("ui-sound-exclude", ctx => ctx.soundExclude("voice.lin", ctx.value));

// Frame video tags were removed in v0.9.0: lifecycle callbacks hand off to UI.
on.frameExit("intro", ctx => ctx.open("story-intro"));
on.screenOpen("story-intro", ctx => ctx.sequence("story.intro"));
on.screenClose("story-intro", ctx => ctx.resume());
on.frameEnter("credits", ctx => ctx.open("story-credits"));
on.screenOpen("story-credits", ctx => ctx.sequence("story.credits"));
on.screenClose("story-credits", ctx => ctx.resume("opening"));
