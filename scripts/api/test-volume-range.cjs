// Run the real preference store with mocked desktop IPC/storage.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const ts = require('typescript');
const storage = new Map([
  ['mixweave-volume-range', JSON.stringify({sink_game:true})],
  ['mixweave-volume-range-master-default', '1'],
]);
let backend = null, fail = false;
const calls = [];
const mocks = {
  zustand: require('zustand'),
  '../types': {MASTER_BUS:'sink_stream',STREAMER_MODE_BUS:'sink_streamer_mode'},
  '@tauri-apps/api/core': {invoke: async (method, args) => {
    calls.push({method,args});
    if (fail) throw new Error('Disk write failed');
    backend ??= {...(args.legacy ?? {})};
    if (args.id) backend[args.id]=args.allowed;
    return {...backend};
  }},
};
const source = fs.readFileSync('src/store/volumeRange.ts','utf8');
const compiled = ts.transpileModule(source, {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
const exported = {};
vm.runInNewContext(compiled, {exports:exported,require:name=>mocks[name],localStorage:{
  getItem:key=>storage.get(key) ?? null,setItem:(key,value)=>storage.set(key,value),
}});
(async()=>{
  await exported.initializeVolumeRanges();
  assert.equal(backend.sink_game,true);
  assert.equal(exported.getVolumeCeiling('sink_game',150),150);
  assert.equal(exported.getVolumeCeiling('sink_chat',150),100);
  await exported.useVolumeRange.getState().setAllowed('sink_game',false);
  assert.equal(backend.sink_game,false);
  assert.equal(exported.getVolumeCeiling('sink_game',150),100);
  storage.set('mixweave-volume-range',JSON.stringify({sink_game:true}));
  await exported.initializeVolumeRanges();
  assert.equal(exported.getVolumeCeiling('sink_game',150),100, 'backend choice wins over stale localStorage');
  fail=true;
  await assert.rejects(exported.useVolumeRange.getState().setAllowed('sink_game',true));
  assert.equal(exported.getVolumeCeiling('sink_game',150),100, 'failed save cannot unlock amplification');
  assert.ok(calls.every(c=>c.method==='sync_volume_ranges'));
  console.log('PASS: legacy migration, backend authority, per-strip limits, toggle persistence, and failed-save behavior.');
})().catch(error=>{console.error(error);process.exitCode=1;});
