"use strict";

on.advance("opening", () => "question");

on.choice("question", "accept", ctx => {
  ctx.add("trust", 1);
  ctx.set("met", true);
  ctx.set("mood", "期待");
  return "walk";
}, ctx => ctx.vars.trust < 3n);

on.choice("question", "refuse", () => "decision");

on.choice("question", "invite", ctx => {
  ctx.set("invited", true);
  return "decision";
}, ctx => ctx.vars.trust >= 2n && ctx.vars.met);

on.advance("walk", () => "question");
on.advance("decision", finishDay);

on.end("accept");
on.end("friendly");
on.end("refuse");

// Each action has an explicit, independently restorable final pose.
on.choice("question", "portrait-demo", () => "actor.stage");
const actorStages = ["stage", "left", "right", "up", "down", "exit-left", "exit-right", "fade", "shake", "bounce", "turn", "spin", "fall-left", "fall-right", "empty", "fade-out", "exit-up", "exit-down"];
for (const stage of actorStages) {
  for (const target of actorStages) on.choice(`actor.${stage}`, target, () => `actor.${target}`);
  on.choice(`actor.${stage}`, "return", () => "question");
}

on.choice("question", "video-demo", () => "intro");
on.choice("question", "credits-demo", () => "credits");
on.advance("intro", () => "opening");
on.end("credits");

// Live2D test scenes remain fully restorable XML frames.
on.choice("question", "live2d-demo", () => "live2d.stage");
for (const stage of ["stage", "expression", "angle"]) {
  const frame = `live2d.${stage}`;
  on.choice(frame, "idle", () => "live2d.stage");
  on.choice(frame, "tap", () => "live2d.stage");
  on.choice(frame, "expression", () => "live2d.expression");
  on.choice(frame, "angle", () => "live2d.angle");
  on.choice(frame, "return", () => "question");
}
