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
on.ui("restart", ctx => ctx.restart());

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
});

on.ui("text-speed", ctx => ctx.setting("text_speed", ctx.value));
on.ui("mute", ctx => ctx.setting("muted", ctx.value));
on.ui("bgm-volume", ctx => ctx.setting("bgm_volume", ctx.value));
on.ui("voice-volume", ctx => ctx.setting("voice_volume", ctx.value));
on.ui("se-volume", ctx => ctx.setting("se_volume", ctx.value));

on.ui("view-cg", ctx => {
  if (!ctx.item.unlocked) return;
  ctx.setUi("cgImage", ctx.item.image);
  ctx.setUi("cgTitle", ctx.item.title);
  ctx.open("cg-view");
});
on.ui("game.enter", ctx => {
  if (ctx.ui.fxTransition) ctx.transition(ctx.ui.fxTransition, Number(ctx.ui.fxDuration));
  ctx.setUi("portraitMode", ctx.game.frameId.startsWith("actor."));
  if (ctx.game.frameId === "accept") ctx.unlock("cg.lin");
});

on.ui("open-effects", ctx => ctx.open("effects"));
on.ui("fx-duration", ctx => ctx.setUi("fxDuration", Math.round(ctx.value)));
on.ui("fx-pause", ctx => {ctx.setUi("fxTransition","fade-white");ctx.setUi("fxDuration",5000);ctx.close();ctx.open("history");ctx.restart();});
on.ui("fx-skip-paused", ctx => {ctx.close();ctx.advance();});
for (const [event, kind] of [["fx-fade","fade-in"],["fx-zoom","zoom-in"],["fx-shake","shake"],["fx-flash","flash"],["fx-slide","slide-left"]]) {
  on.ui(event, ctx => ctx.animate("effectsPreview", kind, 1800, 36));
}
on.ui("fx-scene", ctx => {ctx.close();ctx.animate("scene","shake",1800,24);});
for (const [event, kind] of [["fx-dissolve","dissolve"],["fx-black","fade-black"],["fx-white","fade-white"],
  ...["left","right","up","down"].map(d=>[`fx-wipe-${d}`,`wipe-${d}`]),
  ...["left","right","up","down"].map(d=>[`fx-slide-${d}`,`slide-${d}`]),["fx-default",""]]) {
  on.ui(event, ctx => {ctx.setUi("fxTransition",kind);ctx.close();ctx.restart();});
}

on.ui("open-portrait-stage", ctx => ctx.open("portrait-stage"));
on.ui("portrait-hud", ctx => ctx.open("hud"));
const actorReplay = {"actor.stage": "shake", "actor.left": "enter-left", "actor.right": "enter-right", "actor.up": "enter-up", "actor.down": "enter-down", "actor.exit-left": "exit-left", "actor.exit-right": "exit-right", "actor.fade": "fade-in", "actor.shake": "shake", "actor.bounce": "bounce", "actor.turn": "turn", "actor.spin": "spin", "actor.fall-left": "fall-left", "actor.fall-right": "fall-right", "actor.fade-out": "fade-out", "actor.exit-up": "exit-up", "actor.exit-down": "exit-down"};
on.ui("portrait-replay", ctx => {
  const action = actorReplay[ctx.game.frameId];
  if (action) ctx.portrait("center", action, 1800, 36);
});

on.ui("toggle-auto", ctx => ctx.autoPlay(!ctx.game.autoPlaying));
on.ui("skip-video", ctx => ctx.skipVideo());
on.ui("slots-prev", ctx => ctx.slotsPage(ctx.saves.page - 1));
on.ui("slots-next", ctx => ctx.slotsPage(ctx.saves.page + 1));
on.ui("slots-last", ctx => ctx.slotsPage(20));
on.ui("slots-page", ctx => ctx.slotsPage(Math.round(ctx.value)));
on.ui("open-autosaves", ctx => ctx.open("autosaves"));
on.ui("load-auto-slot", ctx => {ctx.loadAuto(ctx.item.slot);ctx.setUi("status","自动存档读取完成");ctx.close();ctx.close();});
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
