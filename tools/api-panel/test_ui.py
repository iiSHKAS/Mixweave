"""Offscreen widget/HTTP workflow smoke test; does not touch real audio."""
import json
import os
os.environ.setdefault('QT_QPA_PLATFORM','offscreen')
from pathlib import Path
from PyQt6.QtWidgets import QApplication, QSpinBox
from main import Panel
app=QApplication([])
panel=Panel()
root=Path(__file__).resolve().parents[2]
catalog=json.loads((root/'src-tauri/src/local_api/commands.json').read_text())
mic={'node_name':'sink_mic','output_label':'Test mic','gain_percent':100,'muted':False,'input_device':None}
snapshot={'active_profile':'Test','profiles':[{'name':'Test'}], 'channels':[{'name':'sink_game','label':'Game','volume_percent':50,'muted':False}], 'buses':[{'name':'sink_master','label':'Master','volume_percent':100,'muted':False}], 'mic_configs':[mic], 'output_devices':[{'name':'out','description':'Test Output'}], 'input_devices':[{'name':'in','description':'Test Input'}], 'channel_outputs':{},'eq_configs':{'sink_game':{}}}
panel.connected({'catalog':catalog,'snapshot':snapshot,'hardware':[{'name':'out','description':'Test Output','kind':'output','is_default':True,'volume_percent':100,'muted':False}]})
for i in range(panel.methods.count()):
    panel.methods.setCurrentIndex(i)
    assert isinstance(json.loads(panel.params.toPlainText()),dict)
assert panel.controls_layout.count()==5
def maxima():
    return [panel.controls_layout.itemAt(i).widget().findChild(QSpinBox).maximum() for i in range(1,5)]
assert maxima()==[100,100,100,100]
snapshot['prefs']={'volume_range_overrides':{'sink_game':True,'sink_mic':True}}
panel.build_controls([{'name':'out','description':'Test Output','kind':'output','is_default':True,'volume_percent':100,'muted':False}])
assert maxima()==[150,100,200,100]
panel.show();app.processEvents()
panel.grab().save('/tmp/mixweave-api-panel.png')
panel.close()
print(f'PASS: Qt controls and parameter templates for {len(catalog)} methods')
