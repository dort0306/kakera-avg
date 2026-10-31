frame('opening').choice('finish', 'ending');
frame('ending').end();

ui('start').bind('start', ctx => { ctx.open('hud'); ctx.resume('opening'); });
ui('next').bind('advance', ctx => ctx.advance());
ui('choice').bind('choose', ctx => ctx.choose(ctx.item.id));
ui('save').bind('save', ctx => ctx.save(1));
function loadSlot(ctx) { ctx.open('hud'); ctx.load(1); }
ui('load').bind('load', loadSlot);
ui('titleLoad').bind('load', loadSlot);
function quitGame(ctx) { ctx.quit(); }
ui('quit').bind('quit', quitGame);
ui('titleQuit').bind('quit', quitGame);
ui('languages').bind('screen.open', ctx => {
    ctx.setUi('draftText', ctx.localeInfo.textRequested || '');
    ctx.setUi('draftVoice', ctx.localeInfo.voiceRequested || '');
});
ui('textLocale').bind('text-language', ctx => ctx.setUi('draftText', ctx.item.id));
ui('voiceLocale').bind('voice-language', ctx => ctx.setUi('draftVoice', ctx.item.id));
ui('autoText').bind('auto-text', ctx => ctx.setUi('draftText', ''));
ui('autoVoice').bind('auto-voice', ctx => ctx.setUi('draftVoice', ''));
ui('apply').bind('apply', ctx => {
    ctx.locale('text', ctx.ui.draftText || null);
    ctx.locale('voice', ctx.ui.draftVoice || null);
    ctx.close();
});
