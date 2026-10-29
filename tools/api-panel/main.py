#!/usr/bin/env python3
"""Mixweave local API test console. All audio access goes through HTTP."""
import json
import sys
import time
from functools import partial
from PyQt6.QtCore import QObject, QRunnable, QThreadPool, pyqtSignal
from PyQt6.QtWidgets import (QApplication, QMainWindow, QWidget, QVBoxLayout,
    QHBoxLayout, QLineEdit, QPushButton, QLabel, QTabWidget,
    QPlainTextEdit, QComboBox, QSpinBox, QCheckBox, QScrollArea, QGroupBox,
    QMessageBox, QFileDialog)
from client import Client


def pretty(value):
    return json.dumps(value, indent=2, ensure_ascii=False)


class Signals(QObject):
    done = pyqtSignal(object, object, float)


class Job(QRunnable):
    def __init__(self, fn):
        super().__init__()
        self.fn, self.signals = fn, Signals()

    def run(self):
        start = time.monotonic()
        try:
            self.signals.done.emit(self.fn(), None, time.monotonic() - start)
        except Exception as exc:
            self.signals.done.emit(None, str(exc), time.monotonic() - start)


class Panel(QMainWindow):
    def __init__(self):
        super().__init__()
        self.setWindowTitle('Mixweave • Local API Console')
        self.resize(1180, 850)
        self.pool = QThreadPool(self)
        self.pool.setMaxThreadCount(1)
        self.jobs = set()
        self.client = None
        self.snapshot = {}
        self.catalog = []
        self.reports = []
        root = QWidget(); self.setCentralWidget(root)
        layout = QVBoxLayout(root)
        title = QLabel('Mixweave  /  API Console'); title.setStyleSheet('font-size:24px;font-weight:600;padding:8px')
        layout.addWidget(title)
        connection = QHBoxLayout(); layout.addLayout(connection)
        self.url = QLineEdit(); self.url.setPlaceholderText('http://127.0.0.1:PORT')
        self.token = QLineEdit(); self.token.setEchoMode(QLineEdit.EchoMode.Password); self.token.setPlaceholderText('Bearer token')
        connection.addWidget(self.url); connection.addWidget(self.token)
        self.button(connection, 'Discover', self.discover)
        self.button(connection, 'Connect / Refresh', self.connect_api)
        self.status = QLabel('Start the API-enabled Mixweave build, then Discover and Connect.'); layout.addWidget(self.status)
        self.tabs = QTabWidget(); layout.addWidget(self.tabs, 1)
        self.controls = QWidget(); self.controls_layout = QVBoxLayout(self.controls)
        scroll = QScrollArea(); scroll.setWidgetResizable(True); scroll.setWidget(self.controls)
        self.tabs.addTab(scroll, 'Mixer & devices')
        explorer = QWidget(); ex = QVBoxLayout(explorer)
        self.methods = QComboBox(); self.methods.setMinimumWidth(300)
        ex.addWidget(self.methods)
        self.signature = QLabel(); self.signature.setWordWrap(True); ex.addWidget(self.signature)
        self.params = QPlainTextEdit('{}'); ex.addWidget(self.params)
        row = QHBoxLayout(); ex.addLayout(row)
        self.allow_write = QCheckBox('Enable mutating request'); row.addWidget(self.allow_write)
        self.button(row, 'Fill from current state', self.fill_params)
        self.button(row, 'Send request', self.send_explorer)
        self.methods.currentIndexChanged.connect(self.fill_params)
        self.tabs.addTab(explorer, 'All API commands')
        tests = QWidget(); tl = QVBoxLayout(tests)
        tl.addWidget(QLabel('Diagnostics read current state and check authentication/validation.\nAudio writes are tested deliberately using the controls or command explorer.'))
        self.button(tl, 'Run read-only diagnostics', self.diagnostics)
        self.button(tl, 'Save JSON report', self.save_report)
        self.results = QPlainTextEdit(); self.results.setReadOnly(True); tl.addWidget(self.results)
        self.tabs.addTab(tests, 'Diagnostics')
        self.log = QPlainTextEdit(); self.log.setReadOnly(True); self.log.setMaximumHeight(190)
        layout.addWidget(self.log)
        self.discover()

    @staticmethod
    def button(layout, label, fn):
        b = QPushButton(label); b.clicked.connect(fn); layout.addWidget(b); return b

    def discover(self):
        try:
            client = Client.discover()
            self.url.setText(client.url); self.token.setText(client.token)
            self.status.setText('Endpoint discovered. Connect to inspect live state.')
        except Exception as exc:
            self.status.setText(f'Discovery unavailable: {exc}')

    def submit(self, label, fn, callback=None):
        job = Job(fn); self.jobs.add(job)
        self.status.setText(label + '…')
        def complete(value, error, elapsed):
            self.jobs.discard(job)
            self.status.setText(f'{label}: {"FAILED" if error else "OK"} ({elapsed:.2f}s)')
            entry = {'action': label, 'ok': error is None, 'seconds': round(elapsed, 3), 'error': error}
            self.reports.append(entry)
            self.log.appendPlainText(f'{label} ({elapsed:.2f}s)\n' + (error or pretty(value)))
            if len(self.log.toPlainText()) > 80000:
                self.log.setPlainText(self.log.toPlainText()[-60000:])
            if not error and callback:
                callback(value)
        job.signals.done.connect(complete)
        self.pool.start(job)

    def connect_api(self):
        if self.jobs:
            self.status.setText('Wait for the current requests before reconnecting.'); return
        try:
            self.client = Client(self.url.text().strip(), self.token.text().strip())
        except Exception as exc:
            self.status.setText(str(exc)); return
        client = self.client
        def fetch():
            health = client.request('/v1/health')
            catalog = client.request('/v1/commands')
            snapshot = client.snapshot()
            snapshot['prefs'] = client.rpc('get_prefs')
            try:
                hardware = client.rpc('get_hardware_devices')
            except Exception as exc:
                hardware = []; snapshot['hardware_error'] = str(exc)
            return {'health': health, 'catalog': catalog, 'snapshot': snapshot, 'hardware': hardware}
        self.submit('Connect', fetch, self.connected)

    def connected(self, data):
        self.catalog = data['catalog']; self.snapshot = data['snapshot']
        self.methods.blockSignals(True); self.methods.clear()
        for entry in self.catalog:
            self.methods.addItem(f"{entry['group']} / {entry['method']}", entry)
        self.methods.blockSignals(False)
        self.build_controls(data['hardware']); self.fill_params()

    def refresh(self):
        client = self.client
        def fetch():
            snapshot = client.snapshot()
            snapshot['prefs'] = client.rpc('get_prefs')
            try: hardware = client.rpc('get_hardware_devices')
            except Exception as exc:
                hardware = []; snapshot['hardware_error'] = str(exc)
            return snapshot, hardware
        def update(data):
            self.snapshot = data[0]; self.build_controls(data[1])
        self.submit('Refresh state', fetch, update)

    def command(self, method, params):
        if not self.client: return
        client = self.client
        self.submit(method, lambda: client.rpc(method, **params), lambda _: self.refresh())

    def build_controls(self, hardware):
        while self.controls_layout.count():
            item = self.controls_layout.takeAt(0)
            if item.widget(): item.widget().deleteLater()
        profile = self.snapshot.get('active_profile')
        guard = {'expected_profile': profile}
        prefs = self.snapshot.get('prefs', {})
        ranges = prefs.get('volume_range_overrides') or {}
        def maximum(name, hard_max=150):
            key = 'sink_stream' if name == 'sink_streamer_mode' else name
            return hard_max if ranges.get(key, False) else 100
        group = QGroupBox('Profiles'); line = QHBoxLayout(group)
        profiles = QComboBox()
        for p in self.snapshot.get('profiles', []): profiles.addItem(p['name'])
        profiles.setCurrentText(profile or '')
        line.addWidget(profiles)
        self.button(line, 'Switch profile', lambda: self.command('load_profile', {'name': profiles.currentText()}))
        self.button(line, 'Refresh', self.refresh)
        self.controls_layout.addWidget(group)
        for channel in self.snapshot.get('channels', []):
            group = QGroupBox(channel['label']); row = QHBoxLayout(group)
            name = channel['name']
            self.level_controls(row, channel['volume_percent'], channel['muted'], 'set_channel_volume', 'toggle_channel_mute', {'sink_name': name, **guard}, maximum=maximum(name))
            outputs = QComboBox(); outputs.addItem('Follow system default', '')
            for device in self.snapshot.get('output_devices', []): outputs.addItem(device['description'], device['name'])
            outputs.setCurrentIndex(max(0, outputs.findData(self.snapshot.get('channel_outputs', {}).get(name) or '')))
            row.addWidget(outputs)
            self.button(row, 'Set output', partial(self.choose_output, name, outputs, guard))
            self.controls_layout.addWidget(group)
        for bus in self.snapshot.get('buses', []):
            group = QGroupBox('Mix / ' + bus['label']); row = QHBoxLayout(group)
            if bus['name'] == 'sink_streamer_mode' and not prefs.get('streamer_mode_enabled'):
                group.setEnabled(False); group.setTitle('Streamer Mode is disabled — enable it in Mixweave')
            self.level_controls(row, bus['volume_percent'], bus['muted'], 'set_bus_volume', 'set_bus_mute', {'name': bus['name'], **guard}, maximum=maximum(bus['name']))
            self.controls_layout.addWidget(group)
        for mic in self.snapshot.get('mic_configs', []):
            group = QGroupBox('Microphone / ' + mic['output_label']); row = QHBoxLayout(group)
            gain = QSpinBox(); gain.setRange(0,maximum(mic['node_name'], 200)); gain.setSuffix('%'); gain.setValue(mic['gain_percent']); row.addWidget(gain)
            self.button(row, 'Set gain', partial(self.mic_gain, mic, gain, guard))
            self.button(row, 'Unmute' if mic['muted'] else 'Mute', partial(self.mic_patch, mic, {'muted': not mic['muted']}, guard))
            inputs = QComboBox(); inputs.addItem('Follow system default', None)
            for device in self.snapshot.get('input_devices', []): inputs.addItem(device['description'], device['name'])
            inputs.setCurrentIndex(max(0,inputs.findData(mic.get('input_device')))); row.addWidget(inputs)
            self.button(row, 'Set capture input', partial(self.mic_input, mic, inputs, guard))
            self.controls_layout.addWidget(group)
        for device in hardware:
            group = QGroupBox(f"Hardware {device['kind']} / {device['description']}" + (' • default' if device['is_default'] else ''))
            row = QHBoxLayout(group)
            self.level_controls(row, device['volume_percent'], device['muted'], 'set_hardware_volume', 'set_hardware_mute', {'kind':device['kind'], 'name':device['name']}, volume_key='percent', maximum=100)
            self.button(row, 'Make system default', partial(self.command, 'set_default_' + device['kind'], {'name':device['name']}))
            self.controls_layout.addWidget(group)
        if self.snapshot.get('hardware_error'):
            self.controls_layout.addWidget(QLabel('Hardware controls unavailable: ' + self.snapshot['hardware_error']))

    def level_controls(self, row, value, muted, volume_method, mute_method, params, volume_key='volume', maximum=150):
        spin = QSpinBox(); spin.setRange(0, maximum); spin.setValue(min(value, maximum)); spin.setSuffix('%'); row.addWidget(spin)
        self.button(row, '−5', lambda: self.command(volume_method, {**params, volume_key:max(0, spin.value()-5)}))
        self.button(row, 'Apply', lambda: self.command(volume_method, {**params, volume_key:spin.value()}))
        self.button(row, '+5', lambda: self.command(volume_method, {**params, volume_key:min(maximum, spin.value()+5)}))
        self.button(row, 'Unmute' if muted else 'Mute', partial(self.command, mute_method, {**params, 'muted':not muted}))

    def choose_output(self, name, combo, guard): self.command('set_channel_output', {'sink_name':name,'output_name':combo.currentData(),**guard})
    def mic_patch(self, mic, patch, guard): self.command('set_mic_config', {'config':{**mic, **patch}, **guard})
    def mic_gain(self, mic, gain, guard): self.mic_patch(mic, {'gain_percent':gain.value()}, guard)
    def mic_input(self, mic, combo, guard): self.mic_patch(mic, {'input_device':combo.currentData()}, guard)

    def fill_params(self):
        entry = self.methods.currentData()
        if not entry: return
        self.allow_write.setChecked(False)
        self.signature.setText(('READ' if entry['read_only'] else 'WRITE — changes live audio / saved configuration') + '\n' + ', '.join(f"{p['name']}: {p['type']}" for p in entry['params']))
        params = {}
        channels = self.snapshot.get('channels', [])
        name = channels[0]['name'] if channels else 'sink_game'
        for p in entry['params']:
            key, typ = p['name'], p['type']
            if key == 'expected_profile': value = self.snapshot.get('active_profile')
            elif key == 'sink_name': value = name
            elif key == 'node_name': value = (self.snapshot.get('mic_configs') or [{}])[0].get('node_name')
            elif key == 'config' and typ == 'MicConfig': value = (self.snapshot.get('mic_configs') or [{}])[0]
            elif key == 'config' and typ == 'EqConfig': value = self.snapshot.get('eq_configs', {}).get(name, {})
            elif not p['required']: value = None
            elif typ == 'bool': value = False
            elif typ.startswith('Vec<'): value = []
            elif typ in ('u8', 'u32', 'i32', 'f32'): value = 100 if key in ('volume','percent') else 0
            elif typ == 'String': value = ''
            else: value = {}
            params[key] = value
        self.params.setPlainText(pretty(params))

    def send_explorer(self):
        entry = self.methods.currentData()
        if not entry or not self.client: return
        if not entry['read_only'] and not self.allow_write.isChecked():
            self.status.setText('Enable mutating request before sending a write.'); return
        try:
            params = json.loads(self.params.toPlainText())
            if not isinstance(params, dict): raise ValueError('Parameters must be a JSON object')
        except ValueError as exc:
            self.status.setText(str(exc)); return
        self.command(entry['method'], params)

    def diagnostics(self):
        if not self.client: return
        client = self.client
        def run():
            results = []
            for method in ('get_backend_info','get_profile_snapshot','get_virtual_devices','get_app_streams','get_output_devices','get_input_devices','get_hardware_devices','get_default_devices','get_mic_configs','get_channel_outputs','get_channel_eq_configs','get_prefs','list_profiles','list_buses'):
                try:
                    result = client.rpc(method)
                    results.append({'method':method,'ok':True,'result':result})
                except Exception as exc: results.append({'method':method,'ok':False,'error':str(exc)})
            for label, fn, expected in (
                ('Reject wrong token', lambda: Client(client.url,'invalid').request('/v1/health'), 'HTTP 401'),
                ('Reject unknown method', lambda: client.rpc('not_a_method'), 'HTTP 404'),
                ('Reject missing volume', lambda: client.rpc('set_channel_volume',sink_name='sink_game'), 'HTTP 400')):
                try: fn(); results.append({'method':label,'ok':False,'error':'Unexpected success'})
                except Exception as exc: results.append({'method':label,'ok':expected in str(exc),'result':str(exc)})
            return results
        self.submit('Diagnostics', run, lambda result: self.results.setPlainText(pretty(result)))

    def save_report(self):
        path, _ = QFileDialog.getSaveFileName(self,'Save report','mixweave-api-report.json','JSON (*.json)')
        if path:
            try:
                with open(path,'w') as output: output.write(pretty({'requests':self.reports,'diagnostics':self.results.toPlainText()}))
            except OSError as exc: QMessageBox.warning(self,'Save failed',str(exc))

    def closeEvent(self, event):
        if self.jobs:
            self.status.setText('Wait for pending requests before closing; a submitted write may still be running.')
            event.ignore()
        else: event.accept()


if __name__ == '__main__':
    app = QApplication(sys.argv)
    app.setStyle('Fusion')
    app.setStyleSheet('''QWidget{background:#22272e;color:#e3e8ef;font-size:13px} QLineEdit,QPlainTextEdit,QSpinBox,QComboBox{background:#171c22;border:1px solid #46505d;border-radius:5px;padding:6px} QPushButton{background:#35404c;border:1px solid #566271;border-radius:5px;padding:7px 12px} QPushButton:hover{border-color:#ff905d} QGroupBox{border:1px solid #46505d;border-radius:7px;margin-top:14px;padding-top:14px} QGroupBox::title{color:#ff995f} QTabBar::tab{padding:10px 18px} QTabBar::tab:selected{color:#ff995f}''')
    panel = Panel(); panel.show()
    sys.exit(app.exec())
