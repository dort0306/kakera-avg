"use strict";

// A reusable story calculation, independent of XML presentation and UI code.
function finishDay(ctx) {
  if (ctx.vars.invited && ctx.vars.trust >= 2n) {
    ctx.set("mood", "开心");
    return "accept";
  }
  if (ctx.vars.trust >= 1n) {
    ctx.set("mood", "平静");
    return "friendly";
  }
  ctx.set("mood", "安静");
  return "refuse";
}
