frame('opening').choice('finish', 'ending');
frame('ending').end();

event.register('start', ctx => { ctx.open('hud'); ctx.resume('opening'); });
event.register('advance', ctx => ctx.advance());
event.register('choose', ctx => ctx.choose(ctx.item.id));
event.register('save', ctx => ctx.save(1));
function loadSlot(ctx) { ctx.open('hud'); ctx.load(1); }
event.register('load', loadSlot);
function quitGame(ctx) { ctx.quit(); }
event.register('quit', quitGame);
ui('languages').bind('screen.open', ctx => {
    ctx.setUi('draftText', ctx.localeInfo.textRequested || '');
    ctx.setUi('draftVoice', ctx.localeInfo.voiceRequested || '');
});
event.register('text-language', ctx => ctx.setUi('draftText', ctx.item.id));
event.register('voice-language', ctx => ctx.setUi('draftVoice', ctx.item.id));
event.register('auto-text', ctx => ctx.setUi('draftText', ''));
event.register('auto-voice', ctx => ctx.setUi('draftVoice', ''));
event.register('apply', ctx => {
    ctx.locale('text', ctx.ui.draftText || null);
    ctx.locale('voice', ctx.ui.draftVoice || null);
    ctx.close();
});
