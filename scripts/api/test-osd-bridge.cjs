// Exercise the actual React hook with Tauri mocked; no audio/session changes.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const effects = [], listeners = new Map(), emitted = [], invoked = [];
const shortcuts = Object.assign(select => select({enabled:false,bindings:{}}), {getState:()=>({enabled:false})});
const mocks = {
  react: {useEffect: fn => effects.push(fn)},
  '@tauri-apps/plugin-global-shortcut': {},
  '@tauri-apps/api/core': {invoke: async (method, payload) => { invoked.push([method,payload]); return method === 'detect_shortcut_backend' ? 'gnome_wayland' : undefined; }},
  '@tauri-apps/api/event': {emit: async (name,payload) => emitted.push([name,payload]), listen: async (name,fn) => {listeners.set(name,fn); return () => listeners.delete(name);}},
  '../store/mixer': {useMixerStore: {setState:()=>{}}},
  '../store/shortcuts': {DEFAULT_SHORTCUTS:{},useShortcutSettings:shortcuts},
  '../store/channelShortcuts': {useChannelShortcuts:select=>select({byChannel:{}})},
  '../types': {MASTER_BUS:'sink_stream',MAX_VOLUME:150,MAX_MIC_GAIN:200},
  '../store/volumeRange': {getVolumeCeiling:()=>100},
  '../store/theme': {currentLook:()=> 'dark'},
  '../store/osdStyle': {readOsdStyle:()=> 'fader'},
  '../store/osdPosition': {readOsdPosition:()=> 'right-center'},
  '../i18nCore': {translate:(_,key)=>({'mixer.group.master':'Master','streamer.stream':'Stream'}[key] ?? key)},
};
const source=fs.readFileSync(path.join(__dirname,'../../src/hooks/useGlobalShortcuts.ts'),'utf8');
const compiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
const exportsObject={};
vm.runInNewContext(compiled,{exports:exportsObject,require:name=>{assert.ok(name in mocks, name);return mocks[name];},document:{documentElement:{lang:'en'}}});
const flush=()=>new Promise(resolve=>setImmediate(resolve));
(async()=>{
  exportsObject.useGlobalShortcuts();
  const cleanup=effects.map(fn=>fn()).filter(Boolean);
  await flush();
  const notify=listeners.get('local-api-osd');
  assert.equal(typeof notify,'function');
  notify({payload:{id:'sink_stream',label:'Stored master name',volumePercent:65,max:150,muted:true,stream:false}});
  await flush();
  assert.equal(emitted.length,1);
  assert.equal(emitted[0][0],'shortcut-osd');
  assert.equal(emitted[0][1].label,'Master');
  assert.equal(emitted[0][1].max,100);
  assert.equal(emitted[0][1].muted,true);
  const popup=invoked.find(([method])=>method==='show_gnome_osd')[1];
  assert.equal(popup.volumePercent,65);
  assert.equal(popup.theme,'dark');
  assert.equal(popup.style,'fader');
  assert.equal(popup.position,'right-center');
  notify({payload:{id:'sink_game',label:'Game',volumePercent:125,max:150,muted:false,stream:true}});
  await flush();
  assert.equal(emitted[1][1].label,'Game • Stream');
  assert.equal(emitted[1][1].max,150);
  cleanup.forEach(fn=>fn());
  notify({payload:{id:null,label:'Ignored after unmount',volumePercent:1,max:100,muted:false,stream:false}});
  await flush();
  assert.equal(emitted.length,2);
  console.log('PASS: API OSD event reaches X11 event and GNOME bridge with shortcuts disabled; style, position, labels, range and cleanup preserved.');
})().catch(error=>{console.error(error);process.exitCode=1;});
