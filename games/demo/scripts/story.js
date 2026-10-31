"use strict";

frame("opening").advance(() => "question");

frame("question").choice("accept", ctx => {
  ctx.add("trust", 1);
  ctx.set("met", true);
  ctx.set("mood", "期待");
  return "walk";
}, ctx => ctx.vars.trust < 3n);

frame("question").choice("refuse", () => "decision");

frame("question").choice("invite", ctx => {
  ctx.set("invited", true);
  return "decision";
}, ctx => ctx.vars.trust >= 2n && ctx.vars.met);

frame("walk").advance(() => "question");
frame("decision").advance(finishDay);

frame("accept").end();
frame("friendly").end();
frame("refuse").end();

// Each action has an explicit, independently restorable final pose.
frame("question").choice("portrait-demo", () => "actor.stage");
const actorStages = ["stage", "left", "right", "up", "down", "exit-left", "exit-right", "fade", "shake", "bounce", "turn", "spin", "fall-left", "fall-right", "empty", "fade-out", "exit-up", "exit-down"];
for (const stage of actorStages) {
  for (const target of actorStages) frame(`actor.${stage}`).choice(target, () => `actor.${target}`);
  frame(`actor.${stage}`).choice("return", () => "question");
}

frame("question").choice("video-demo", () => "intro");
frame("question").choice("credits-demo", () => "credits");
frame("intro").advance(() => "opening");
frame("credits").end();

// Live2D test scenes remain fully restorable XML frames.
frame("question").choice("live2d-demo", () => "live2d.stage");
for (const scene of ["stage", "expression", "angle"]) {
  const id = `live2d.${scene}`;
  frame(id).choice("idle", () => "live2d.stage");
  frame(id).choice("tap", () => "live2d.stage");
  frame(id).choice("expression", () => "live2d.expression");
  frame(id).choice("angle", () => "live2d.angle");
  frame(id).choice("return", () => "question");
}
