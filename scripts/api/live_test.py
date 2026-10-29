"""Live integration suite. Refuses to write unless using the isolated test graph."""
import argparse
import json
import os
from pathlib import Path
import sys
import time
sys.path.insert(0,str(Path(__file__).resolve().parents[2] / 'tools/api-panel'))
from client import Client, ApiError
parser=argparse.ArgumentParser();parser.add_argument('--isolated',required=True);parser.add_argument('--report',required=True)
args=parser.parse_args()
root=Path(args.isolated).resolve()
assert str(root).startswith('/tmp/mixweave-api-test.')
assert Path(os.environ['XDG_CONFIG_HOME']).resolve()==root/'config'
assert os.environ['PULSE_SERVER']==f'unix:{root}/runtime/pulse/native'
results=[]
def check(name,fn):
    start=time.monotonic()
    try:
        result=fn()
        results.append({'test':name,'ok':True,'seconds':round(time.monotonic()-start,3)})
        print('PASS',name,flush=True)
        return result
    except Exception as exc:
        results.append({'test':name,'ok':False,'error':str(exc)})
        print('FAIL',name,str(exc),flush=True)
        raise

def equal(actual,expected):
    assert actual==expected,(actual,expected)

client=None
for _ in range(450):
    try:
        client=Client.discover()
        client.request('/v1/health')
        snapshot=client.snapshot()
        if len(snapshot['channels'])>=4: break
    except Exception: pass
    time.sleep(0.2)
else:
    print((root/'mixweave.log').read_text()[-8000:]);raise SystemExit('API/mixer did not initialize')

def rpc(method,**params): return client.rpc(method,**params)
def rejected(code, method, **params):
    try:
        rpc(method, **params)
    except ApiError as error:
        assert f'HTTP 409: {code}:' in str(error), str(error)
    else:
        raise AssertionError(f'{method} should reject with {code}')

def eventually(fn, expected):
    for _ in range(30):
        actual=fn()
        if actual==expected: return
        time.sleep(0.1)
    equal(actual,expected)
def channel(name): return next(c for c in rpc('get_virtual_devices') if c['name']==name)
try:
    if os.environ.get('MIXWEAVE_TEST_VERSION'):
        check('packaged application version',lambda:equal(client.request('/v1/health')['version'],os.environ['MIXWEAVE_TEST_VERSION']))
    check('native PipeWire backend',lambda:equal(rpc('get_backend_info')['native'],True))
    check('catalog',lambda:equal(len(client.request('/v1/commands')),95))
    snapshot=check('complete profile snapshot',client.snapshot)
    # This assertion prevents accidental writes even if environment isolation fails.
    outputs=rpc('get_output_devices')
    assert all(d['name'].startswith('api_test_') for d in outputs),outputs
    assert len(outputs)>=2
    for name in ['get_hardware_devices','get_default_devices','get_mic_configs','get_prefs','list_profiles','list_buses','get_profile_automation','get_profile_automation_status','list_running_applications']:
        check(name,lambda n=name:rpc(n))
    game=snapshot['channels'][0]['name']
    for volume in (35,40,30,100):
        check(f'channel volume {volume}',lambda v=volume:(rpc('set_channel_volume',sink_name=game,volume=v),equal(channel(game)['volume_percent'],v)))
    def volume_policy_test():
        for volume in (105, 150, 105):
            rpc('set_channel_volume',sink_name=game,volume=volume)
            equal(channel(game)['volume_percent'],100)
        rpc('set_channel_volume',sink_name='sink_media',volume=150)
        equal(channel('sink_media')['volume_percent'],150)
        rejected('volume_limit_exceeded','set_channel_volume',sink_name='sink_media',volume=151)
        rpc('set_channel_volume',sink_name='sink_media',volume=100)
        rpc('set_bus_volume',name='sink_stream',volume=150)
        equal(rpc('get_bus',name='sink_stream')['volume_percent'],100)
        config=rpc('get_mic_config');config['gain_percent']=150
        rejected('volume_limit_exceeded','set_mic_config',config=config)
    check('per-strip amplification limits and enabled 150 percent',volume_policy_test)
    for muted in (True,False):
        check(f'channel mute {muted}',lambda m=muted:(rpc('toggle_channel_mute',sink_name=game,muted=m),equal(channel(game)['muted'],m)))
    check('channel output pin',lambda:(rpc('set_channel_output',sink_name=game,output_name='api_test_out2'),equal(rpc('get_channel_outputs')[game],'api_test_out2')))
    check('channel output default',lambda:(rpc('set_channel_output',sink_name=game,output_name=''),equal(rpc('get_channel_outputs')[game],None)))
    check('default output',lambda:(rpc('set_default_output',name='api_test_out2'),eventually(lambda:rpc('get_default_devices')['output'],'api_test_out2')))
    check('default input',lambda:(rpc('set_default_input',name='api_test_in'),eventually(lambda:rpc('get_default_devices')['input'],'api_test_in')))
    def hardware_test():
        rpc('set_hardware_volume',kind='output',name='api_test_out2',percent=65)
        rpc('set_hardware_mute',kind='output',name='api_test_out2',muted=True)
        device=next(d for d in rpc('get_hardware_devices') if d['name']=='api_test_out2')
        equal(device['volume_percent'],65);equal(device['muted'],True)
        rpc('set_hardware_mute',kind='output',name='api_test_out2',muted=False)
    check('physical output level and mute',hardware_test)
    def mic_test():
        config=rpc('get_mic_config');config.update(enabled=True,gain_percent=75,muted=True,input_device='api_test_in')
        rpc('set_mic_config',config=config)
        actual=rpc('get_mic_config')
        for key in ('gain_percent','muted','input_device'): equal(actual[key],config[key])
        config['muted']=False;rpc('set_mic_config',config=config)
    check('microphone gain mute and input',mic_test)
    def bus_test():
        buses=rpc('list_buses');name=buses[0]['name']
        rpc('set_bus_volume',name=name,volume=70);rpc('set_bus_mute',name=name,muted=True)
        actual=next(b for b in rpc('list_buses') if b['name']==name)
        equal(actual['volume_percent'],70);equal(actual['muted'],True)
        rpc('set_bus_mute',name=name,muted=False)
    check('master mix level and mute',bus_test)
    def disabled_stream_test():
        rpc('set_streamer_mode_enabled',enabled=False)
        before=channel(game)
        rejected('streamer_mode_disabled','set_channel_stream_volume',sink_name=game,volume=45)
        rejected('streamer_mode_disabled','toggle_channel_stream_mute',sink_name=game,muted=True)
        rejected('streamer_mode_disabled','set_bus_volume',name='sink_streamer_mode',volume=70)
        rejected('streamer_mode_disabled','set_bus_mute',name='sink_streamer_mode',muted=True)
        rejected('streamer_mode_disabled','set_monitor',sink_name='sink_mic_stream',enabled=True)
        config=rpc('get_mic_config');config['stream_send_gain_percent']=60
        rejected('streamer_mode_disabled','set_mic_config',config=config)
        equal(channel(game),before)
    check('disabled streamer mode rejects writes without changes',disabled_stream_test)
    rpc('set_streamer_mode_enabled',enabled=True)
    check('stream volume follows channel amplification choice',lambda:(rpc('set_channel_stream_volume',sink_name=game,volume=150),equal(channel(game)['stream_send_volume_percent'],100)))
    check('stream send level',lambda:(rpc('set_channel_stream_volume',sink_name=game,volume=45),equal(channel(game)['stream_send_volume_percent'],45)))
    check('stream send mute',lambda:(rpc('toggle_channel_stream_mute',sink_name=game,muted=True),equal(channel(game)['stream_send_muted'],True)))
    def channels_test():
        rpc('add_channel',label='API Test',icon='music_note',spatial=False)
        c=next(c for c in rpc('get_virtual_devices') if c['label']=='API Test')
        rpc('rename_channel',sink_name=c['name'],label='Renamed API Test')
        equal(channel(c['name'])['label'],'Renamed API Test')
        rpc('remove_channel',sink_name=c['name'])
        assert not any(x['name']==c['name'] for x in rpc('get_virtual_devices'))
    check('channel lifecycle',channels_test)
    def input_hardware_test():
        rpc('set_hardware_volume',kind='input',name='api_test_in',percent=60)
        rpc('set_hardware_mute',kind='input',name='api_test_in',muted=True)
        device=next(d for d in rpc('get_hardware_devices') if d['name']=='api_test_in')
        equal(device['volume_percent'],60);equal(device['muted'],True)
        rpc('set_hardware_mute',kind='input',name='api_test_in',muted=False)
    check('physical input level and mute',input_hardware_test)
    def eq_test():
        config=rpc('import_eq_config',text='Preamp: -3 dB\nFilter 1: ON PK Fc 1000 Hz Gain -2 dB Q 1')
        rpc('set_channel_eq',sink_name=game,config=config)
        equal(rpc('get_channel_eq_configs')[game]['preamp_db'],-3.0)
        rpc('save_user_eq_preset',sink_name=game,name='API EQ',config=config)
        assert any(p['preset']['name']=='API EQ' for p in rpc('list_eq_presets',sink_name=game))
        rpc('export_channel_eq',sink_name=game)
        rpc('delete_user_eq_preset',sink_name=game,name='API EQ')
    check('EQ and preset lifecycle',eq_test)
    def routing_test():
        import subprocess
        with open('/dev/zero','rb') as silence:
            process=subprocess.Popen(['pw-cat','--playback','--target',game,'--properties','application.name=MixweaveAPITest','-'],stdin=silence,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            try:
                for _ in range(30):
                    streams=rpc('get_app_streams')
                    if streams: break
                    time.sleep(0.1)
                assert streams,'no test stream'
                stream=streams[0]
                target=snapshot['channels'][1]['name']
                rpc('route_app_to_channel',stream_index=stream['index'],sink_name=target)
                eventually(lambda:next(x for x in rpc('get_app_streams') if x['index']==stream['index'])['assigned_sink'],target)
                rpc('set_app_volume',stream_index=stream['index'],volume=55)
            finally:
                process.terminate();process.wait(timeout=5)
    check('live application routing and volume',routing_test)
    check('streamer mode on/off',lambda:(rpc('set_streamer_mode_enabled',enabled=True),equal(rpc('get_prefs')['streamer_mode_enabled'],True),rpc('set_streamer_mode_enabled',enabled=False),equal(rpc('get_prefs')['streamer_mode_enabled'],False)))

    def profiles_test():
        rpc('create_blank_profile',name='API Test Profile',mic_enabled=False)
        rpc('load_profile',name='API Test Profile')
        equal(rpc('get_active_profile'),'API Test Profile')
        rpc('copy_profile',source_name='API Test Profile',name='API Copy')
        rpc('rename_profile',name='API Copy',new_name='API Renamed')
        rpc('delete_profile',name='API Renamed')
        try: rpc('set_channel_volume',sink_name=game,volume=10,expected_profile='Wrong profile')
        except Exception as exc: assert 'profile changed' in str(exc)
        else: raise AssertionError('stale profile write accepted')
    check('profile lifecycle and stale-write protection',profiles_test)
finally:
    Path(args.report).write_text(json.dumps(results,indent=2)+'\n')
print(f'{len(results)} live checks passed')
