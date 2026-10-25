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
