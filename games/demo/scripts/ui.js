"use strict";

// Every callback is registered on the real XML target. Events are author names,
// so two controls may keep the same event name while using different handlers.
const pageClosers = ["saveClose", "loadClose", "historyClose", "skinClose", "galleryClose", "autoLoadClose", "effectsClose", "confirmNo", "viewerClose"];
function closePage(ctx) { ctx.close(); }
for (const target of pageClosers) ui(target).bind("close", closePage);

for (const target of ["saveButton", "actorSave"]) ui(target).bind("open-save", ctx => ctx.open("save"));
for (const target of ["loadButton", "actorLoad"]) ui(target).bind("open-load", ctx => ctx.open("load"));
for (const target of ["historyButton", "actorHistory"]) ui(target).bind("open-history", ctx => ctx.open("history"));
ui("galleryButton").bind("open-gallery", ctx => ctx.open("gallery"));

ui("continue").bind("advance", ctx => ctx.advance());
function chooseOption(ctx) {
  if (ctx.item.id === "portrait-demo") ctx.open("portrait-stage");
  if (ctx.item.id === "return" && ctx.game.frameId.startsWith("actor.")) ctx.open("hud");
  if (ctx.game.frameId.startsWith("live2d.") && ctx.item.id === "tap") {
    ctx.live2d("center", "TapBody:0");
  }
  ctx.choose(ctx.item.id);
}
ui("choice").bind("choose", chooseOption);
ui("actorChoice").bind("choose", chooseOption);

function askConfirmation(ctx, action, title, message, acceptText) {
  ctx.setUi("dialogAction", action);
  ctx.setUi("dialogTitle", title);
  ctx.setUi("dialogHasTitle", title.length > 0);
  ctx.setUi("dialogMessage", message);
  ctx.setUi("dialogAcceptText", acceptText);
  ctx.setUi("dialogCancelText", "取消");
  ctx.open("confirmation");
}
ui("restartButton").bind("restart", ctx => askConfirmation(ctx, "restart", "", "返回主菜单并重新选择故事起点？未保存的剧情进度不会保留。", "返回主菜单"));
for (const target of ["quitButton", "titleQuit"]) {
  ui(target).bind("request-quit", ctx => askConfirmation(ctx, "quit", "退出游戏", "确定退出游戏？未保存的剧情进度不会保留。", "退出游戏"));
}
ui("dialogAccept").bind("dialog-accept", ctx => {
  if (ctx.ui.dialogAction === "quit") {
    ctx.quit();
  } else if (ctx.ui.dialogAction === "restart") {
    ctx.close();
    ctx.open("title");
  } else {
    throw new Error("未知确认操作");
  }
});
ui("startMain").bind("start-main", ctx => { ctx.open("hud"); ctx.restart("opening"); });
ui("startOther").bind("start-other", ctx => { ctx.open("hud"); ctx.restart("alternate.opening"); });
frame("alternate.opening").end();

ui("saveSlotButton").bind("save-slot", ctx => {
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
ui("confirmYes").bind("confirm-save", ctx => {
  ctx.save(Number(ctx.ui.pendingSlot), true);
  ctx.setUi("status", "覆盖完成");
  ctx.close();
  ctx.close();
});
ui("loadSlotButton").bind("load-slot", ctx => {
  ctx.load(ctx.item.slot);
  ctx.setUi("status", "读档完成");
  ctx.close();
  ctx.open("hud");
});

ui("speed").bind("text-speed", ctx => ctx.setting("text_speed", ctx.value));
ui("muted").bind("mute", ctx => ctx.setting("muted", ctx.value));
ui("bgm").bind("bgm-volume", ctx => ctx.setting("bgm_volume", ctx.value));
ui("voice").bind("voice-volume", ctx => ctx.setting("voice_volume", ctx.value));
ui("se").bind("se-volume", ctx => ctx.setting("se_volume", ctx.value));

// Shortcut drafts live in UI strings. Only "应用" writes keybindings.json.
const shortcutDrafts = {advance: "shortcutAdvanceDraft", fast_forward: "shortcutFastForwardDraft", skip_sequence: "shortcutSkipDraft"};
const shortcutDefaults = {advance: "Space", fast_forward: "Ctrl", skip_sequence: "Space"};
function beginShortcutCapture(ctx, action) {
  ctx.captureShortcut(action, "shortcut-captured");
  ctx.setUi("shortcutStatus", "请按下新按键；Escape 取消捕获。");
}
ui("shortcutAdvanceCapture").bind("shortcut-capture-advance", ctx => beginShortcutCapture(ctx, "advance"));
ui("shortcutFastCapture").bind("shortcut-capture-fast", ctx => beginShortcutCapture(ctx, "fast_forward"));
ui("shortcutSkipCapture").bind("shortcut-capture-skip", ctx => beginShortcutCapture(ctx, "skip_sequence"));
// The capture result goes to the fixed $host target, never to an arbitrary control.
ui("$host").bind("shortcut-captured", ctx => {
  const draft = shortcutDrafts[ctx.item.action];
  if (!draft) return;
  ctx.setUi(draft, ctx.value);
  ctx.setUi("shortcutStatus", `已捕获 ${ctx.value}；点“应用”保存，或继续更改。`);
});
ui("shortcutAdvanceClear").bind("shortcut-clear-advance", ctx => {
  ctx.setUi("shortcutAdvanceDraft", "");
  ctx.setUi("shortcutStatus", "草稿已清空；点“应用”后该键盘动作禁用。");
});
ui("shortcutFastClear").bind("shortcut-clear-fast", ctx => {
  ctx.setUi("shortcutFastForwardDraft", "");
  ctx.setUi("shortcutStatus", "草稿已清空；点“应用”后该键盘动作禁用。");
});
ui("shortcutSkipClear").bind("shortcut-clear-skip", ctx => {
  ctx.setUi("shortcutSkipDraft", "");
  ctx.setUi("shortcutStatus", "草稿已清空；点“应用”后该键盘动作禁用。");
});
ui("shortcutApply").bind("shortcut-apply", ctx => {
  for (const [action, draft] of Object.entries(shortcutDrafts)) {
    const text = ctx.ui[draft];
    ctx.keybinding(action, text.length === 0 ? null : text);
  }
  ctx.setUi("shortcutStatus", "已应用快捷键并写入独立偏好文件。");
});
ui("shortcutReset").bind("shortcut-reset", ctx => {
  ctx.resetKeybindings();
  for (const [action, draft] of Object.entries(shortcutDrafts)) ctx.setUi(draft, shortcutDefaults[action]);
  ctx.setUi("shortcutStatus", "已恢复默认键位。");
});
ui("shortcutCancel").bind("shortcut-cancel", ctx => {
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
ui("backgroundPicker").bind("color-bg", ctx => ctx.setUi("dialogueBgDraft", ctx.value));
ui("textPicker").bind("color-text", ctx => ctx.setUi("dialogueTextDraft", ctx.value));
ui("colorsApply").bind("color-apply", ctx => {
  for (const slot of ["dialogue_background", "dialogue_text"]) {
    ctx.colorPreference(slot, ctx.ui[colorDrafts[slot]]);
  }
  ctx.setUi("colorStatus", "已应用并写入配色偏好。");
});
ui("colorsReset").bind("color-reset", ctx => {
  ctx.resetColors();
  for (const slot of ["dialogue_background", "dialogue_text"]) ctx.setUi(colorDrafts[slot], colorDefaults[slot]);
  ctx.setUi("colorStatus", "已恢复作者默认配色。");
});
ui("colorsCancel").bind("color-cancel", ctx => {
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
ui("cgOpen").bind("view-cg", ctx => {
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
ui("viewerPrev").bind("cg-prev", ctx => {
  const position = cgPosition(ctx.ui.cgCurrent);
  if (position <= 0) return;
  showCg(ctx, cgBrowse[position - 1], position - 1, ctx.ui.cgToolbar);
});
ui("viewerNext").bind("cg-next", ctx => {
  const position = cgPosition(ctx.ui.cgCurrent);
  if (position < 0 || position + 1 >= cgBrowse.length) return;
  showCg(ctx, cgBrowse[position + 1], position + 1, ctx.ui.cgToolbar);
});
ui("viewerHide").bind("cg-hide-toolbar", ctx => ctx.setUi("cgToolbar", false));
ui("cg-view").bind("cg-background", ctx => {
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

ui("effectsButton").bind("open-effects", ctx => ctx.open("effects"));
ui("fxDurationSlider").bind("fx-duration", ctx => ctx.setUi("fxDuration", Math.round(ctx.value)));
ui("effectsPausePreview").bind("fx-pause", ctx => {ctx.setUi("fxTransition","fade-white");ctx.setUi("fxDuration",5000);ctx.close();ctx.open("history");ctx.restart("opening");});
ui("historySkipTransition").bind("fx-skip-paused", ctx => {ctx.close();ctx.advance();});
const animatedEffects = [
  ["fxFade", "fx-fade", "fade-in"], ["fxZoom", "fx-zoom", "zoom-in"],
  ["fxShake", "fx-shake", "shake"], ["fxFlash", "fx-flash", "flash"],
  ["fxSlide", "fx-slide", "slide-left"],
];
for (const [target, event, kind] of animatedEffects) {
  ui(target).bind(event, ctx => ctx.animate("effectsPreview", kind, 1800, 36));
}
ui("fxScene").bind("fx-scene", ctx => {ctx.close();ctx.animate("scene","shake",1800,24);});
const transitionEffects = [
  ["fxDissolve", "fx-dissolve", "dissolve"], ["fxBlack", "fx-black", "fade-black"],
  ["fxWhite", "fx-white", "fade-white"], ["fxDefault", "fx-default", ""],
  ["fxWipeL", "fx-wipe-left", "wipe-left"], ["fxWipeR", "fx-wipe-right", "wipe-right"],
  ["fxWipeU", "fx-wipe-up", "wipe-up"], ["fxWipeD", "fx-wipe-down", "wipe-down"],
  ["fxSlideL", "fx-slide-left", "slide-left"], ["fxSlideR", "fx-slide-right", "slide-right"],
  ["fxSlideU", "fx-slide-up", "slide-up"], ["fxSlideD", "fx-slide-down", "slide-down"],
];
for (const [target, event, kind] of transitionEffects) {
  ui(target).bind(event, ctx => {ctx.setUi("fxTransition",kind);ctx.close();ctx.restart("opening");});
}

ui("portraitStageButton").bind("open-portrait-stage", ctx => ctx.open("portrait-stage"));
ui("actorHud").bind("portrait-hud", ctx => ctx.open("hud"));
const actorReplay = {"actor.stage": "shake", "actor.left": "enter-left", "actor.right": "enter-right", "actor.up": "enter-up", "actor.down": "enter-down", "actor.exit-left": "exit-left", "actor.exit-right": "exit-right", "actor.fade": "fade-in", "actor.shake": "shake", "actor.bounce": "bounce", "actor.turn": "turn", "actor.spin": "spin", "actor.fall-left": "fall-left", "actor.fall-right": "fall-right", "actor.fade-out": "fade-out", "actor.exit-up": "exit-up", "actor.exit-down": "exit-down"};
ui("actorReplay").bind("portrait-replay", ctx => {
  const action = actorReplay[ctx.game.frameId];
  if (action) ctx.portrait("center", action, 1800, 36);
});

ui("autoPlayButton").bind("toggle-auto", ctx => ctx.autoPlay(!ctx.game.autoPlaying));
for (const target of ["savePrevPage", "loadPrevPage"]) ui(target).bind("slots-prev", ctx => ctx.slotsPage(ctx.saves.page - 1));
for (const target of ["saveNextPage", "loadNextPage"]) ui(target).bind("slots-next", ctx => ctx.slotsPage(ctx.saves.page + 1));
for (const target of ["saveLastPage", "loadLastPage"]) ui(target).bind("slots-last", ctx => ctx.slotsPage(20));
for (const target of ["savePageSlider", "loadPageSlider"]) ui(target).bind("slots-page", ctx => ctx.slotsPage(Math.round(ctx.value)));
ui("autoSavesButton").bind("open-autosaves", ctx => ctx.open("autosaves"));
ui("autoLoadSlotButton").bind("load-auto-slot", ctx => {ctx.loadAuto(ctx.item.slot);ctx.setUi("status","自动存档读取完成");ctx.close();ctx.close();ctx.open("hud");});
ui("autoDelay").bind("auto-delay", ctx => ctx.setting("auto_delay", ctx.value));
ui("autosave").bind("autosave-setting", ctx => ctx.setting("autosave", ctx.value));
ui("skipAll").bind("skip-all", ctx => ctx.setting("skip_all", ctx.value));
ui("videoVolume").bind("video-volume", ctx => ctx.setting("video_volume", ctx.value));
ui("live2dMotion").bind("live2d-tap", ctx => ctx.live2d("center", "TapBody:0"));

// Background input belongs to the real top screen, so both staged pages register.
function backgroundAdvance(ctx) {
  if (!ctx.ui.hudVisible) {ctx.setUi("hudVisible",true);return;}
  if (ctx.game.canAdvance) ctx.advance();
}
function toggleHud(ctx) { ctx.setUi("hudVisible",!ctx.ui.hudVisible); }
for (const target of ["hud", "portrait-stage"]) {
  ui(target).bind("background-advance", backgroundAdvance);
  ui(target).bind("toggle-hud", toggleHud);
}

ui("fullscreen").bind("fullscreen", ctx => ctx.setting("fullscreen", ctx.value));


ui("openSkinDemo").bind("open-skins", ctx => ctx.open("skins"));
ui("windowTitleButton").bind("window-title", ctx => ctx.setUi("windowTitle",
  ctx.ui.windowTitle === "自定义标题 · 中文 & JS" ? `午后物语 · ${ctx.game.frameId}` : "自定义标题 · 中文 & JS"));
const spriteRegions = [["spriteBlueButton", "sprite-blue", "0,0,64,64"], ["spriteGreenButton", "sprite-green", "64,0,64,64"], ["spriteRedButton", "sprite-red", "128,0,64,64"]];
for (const [target, event, region] of spriteRegions) {
  ui(target).bind(event, ctx => ctx.setUi("spriteRegion", region));
}

// Presentation is transient; preferences are independent of story slots.
ui("presentationFade").bind("ui-fade", ctx => ctx.sequence("ui.fade"));
ui("presentationIntro").bind("ui-intro", ctx => ctx.sequence("ui.intro"));
ui("presentationCredits").bind("ui-credits", ctx => ctx.sequence("ui.credits"));
ui("presentationAudio").bind("ui-sound-enabled", ctx => ctx.soundEnabled(ctx.value));
ui("presentationLin").bind("ui-sound-lin", ctx => ctx.soundActor("lin", ctx.value));
ui("presentationExclude").bind("ui-sound-exclude", ctx => ctx.soundExclude("voice.lin", ctx.value));

// Frame video tags were removed in v0.9.0: lifecycle callbacks hand off to UI.
frame("intro").exit(ctx => ctx.open("story-intro"));
ui("story-intro").bind("screen.open", ctx => ctx.sequence("story.intro"));
ui("story-intro").bind("screen.close", ctx => ctx.resume());
frame("credits").enter(ctx => ctx.open("story-credits"));
ui("story-credits").bind("screen.open", ctx => ctx.sequence("story.credits"));
ui("story-credits").bind("screen.close", ctx => ctx.resume("opening"));
