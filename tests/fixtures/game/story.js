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
