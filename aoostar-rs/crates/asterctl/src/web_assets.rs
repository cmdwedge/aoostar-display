// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026

//! Embedded Web Dashboard assets (HTML5, CSS, and vanilla JavaScript).
//! Completely self-contained with zero external CDN dependencies.

pub const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>AOOSTAR WTR Max - Display Control</title>
  <style>
    :root {
      --bg: #0b0f19;
      --card-bg: rgba(22, 27, 34, 0.85);
      --card-border: #30363d;
      --text: #f0f6fc;
      --text-muted: #8b949e;
      --cyan: #00e5ff;
      --cyan-glow: rgba(0, 229, 255, 0.25);
      --orange: #ff9100;
      --orange-glow: rgba(255, 145, 0, 0.25);
      --green: #2ea043;
      --green-glow: rgba(46, 160, 67, 0.25);
      --red: #f85149;
      --radius: 12px;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      background: var(--bg);
      color: var(--text);
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
      min-height: 100vh;
      padding: 24px;
      display: flex;
      flex-direction: column;
      align-items: center;
    }
    .container {
      width: 100%;
      max-width: 1060px;
      display: flex;
      flex-direction: column;
      gap: 24px;
    }
    header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 16px 24px;
      background: var(--card-bg);
      border: 1px solid var(--card-border);
      border-radius: var(--radius);
      backdrop-filter: blur(12px);
    }
    .brand {
      display: flex;
      align-items: center;
      gap: 14px;
    }
    .logo-badge {
      background: linear-gradient(135deg, var(--cyan), #0072ff);
      color: #000;
      font-weight: 800;
      font-size: 14px;
      padding: 6px 12px;
      border-radius: 8px;
      letter-spacing: 1px;
    }
    h1 { font-size: 20px; font-weight: 600; }
    .status-badge {
      display: flex;
      align-items: center;
      gap: 8px;
      font-size: 13px;
      color: var(--green);
      background: var(--green-glow);
      padding: 6px 14px;
      border-radius: 20px;
      font-weight: 600;
    }
    .pulse-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: var(--green);
      box-shadow: 0 0 8px var(--green);
      animation: pulse 2s infinite;
    }
    @keyframes pulse {
      0% { transform: scale(0.95); opacity: 0.8; }
      50% { transform: scale(1.2); opacity: 1; }
      100% { transform: scale(0.95); opacity: 0.8; }
    }

    /* Preview Section */
    .preview-card {
      background: var(--card-bg);
      border: 1px solid var(--card-border);
      border-radius: var(--radius);
      padding: 24px;
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 18px;
    }
    .lcd-bezel {
      background: #000;
      border: 4px solid #22272e;
      border-radius: 14px;
      padding: 6px;
      box-shadow: 0 12px 32px rgba(0,0,0,0.6), 0 0 16px var(--cyan-glow);
      width: 100%;
      max-width: 960px;
      aspect-ratio: 960 / 376;
      display: flex;
      align-items: center;
      justify-content: center;
      overflow: hidden;
      position: relative;
    }
    .lcd-bezel img {
      width: 100%;
      height: 100%;
      object-fit: contain;
      display: block;
      border-radius: 8px;
    }
    .preview-toolbar {
      display: flex;
      flex-wrap: wrap;
      gap: 12px;
      justify-content: center;
      width: 100%;
    }
    button, .btn {
      background: #21262d;
      color: var(--text);
      border: 1px solid var(--card-border);
      padding: 10px 18px;
      border-radius: 8px;
      font-size: 14px;
      font-weight: 600;
      cursor: pointer;
      display: inline-flex;
      align-items: center;
      gap: 8px;
      transition: all 0.2s ease;
    }
    button:hover, .btn:hover {
      background: #30363d;
      border-color: #8b949e;
      transform: translateY(-1px);
    }
    button.btn-primary {
      background: linear-gradient(135deg, #00b4d8, #0077b6);
      border-color: #0077b6;
      color: #fff;
    }
    button.btn-primary:hover {
      box-shadow: 0 0 12px var(--cyan-glow);
    }
    button.btn-amber {
      background: linear-gradient(135deg, #f77f00, #d62828);
      border-color: #d62828;
      color: #fff;
    }
    button.btn-amber:hover {
      box-shadow: 0 0 12px var(--orange-glow);
    }

    /* Grid Section */
    .grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
      gap: 24px;
    }
    .card {
      background: var(--card-bg);
      border: 1px solid var(--card-border);
      border-radius: var(--radius);
      padding: 24px;
      display: flex;
      flex-direction: column;
      gap: 20px;
    }
    .card-title {
      font-size: 16px;
      font-weight: 700;
      display: flex;
      align-items: center;
      gap: 10px;
      color: var(--cyan);
      border-bottom: 1px solid var(--card-border);
      padding-bottom: 12px;
    }
    .form-group {
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
    .form-row {
      display: flex;
      justify-content: space-between;
      align-items: center;
    }
    label {
      font-size: 14px;
      font-weight: 500;
      color: var(--text);
    }
    .hint {
      font-size: 12px;
      color: var(--text-muted);
    }
    input[type="text"], input[type="time"], input[type="number"], select {
      background: #0d1117;
      border: 1px solid var(--card-border);
      color: var(--text);
      padding: 8px 14px;
      border-radius: 6px;
      font-size: 14px;
      width: 100%;
    }
    input:focus, select:focus {
      outline: none;
      border-color: var(--cyan);
      box-shadow: 0 0 8px var(--cyan-glow);
    }
    .toggle-switch {
      position: relative;
      display: inline-block;
      width: 44px;
      height: 24px;
    }
    .toggle-switch input { opacity: 0; width: 0; height: 0; }
    .slider {
      position: absolute; cursor: pointer; top: 0; left: 0; right: 0; bottom: 0;
      background-color: #30363d;
      transition: .3s;
      border-radius: 24px;
    }
    .slider:before {
      position: absolute; content: ""; height: 18px; width: 18px; left: 3px; bottom: 3px;
      background-color: white;
      transition: .3s;
      border-radius: 50%;
    }
    input:checked + .slider { background-color: var(--cyan); }
    input:checked + .slider:before { transform: translateX(20px); }

    .checkbox-group {
      display: flex;
      flex-direction: column;
      gap: 10px;
    }
    .checkbox-item {
      display: flex;
      align-items: center;
      gap: 12px;
      background: #101520;
      padding: 10px 14px;
      border-radius: 8px;
      border: 1px solid var(--card-border);
      cursor: pointer;
    }
    .checkbox-item input { cursor: pointer; }

    /* Floating Save Bar */
    .save-bar {
      position: sticky;
      bottom: 24px;
      background: rgba(22, 27, 34, 0.95);
      border: 1px solid var(--cyan);
      box-shadow: 0 8px 24px rgba(0,0,0,0.7), 0 0 16px var(--cyan-glow);
      border-radius: var(--radius);
      padding: 14px 24px;
      display: flex;
      justify-content: space-between;
      align-items: center;
      backdrop-filter: blur(16px);
      z-index: 100;
    }
    .toast {
      color: var(--green);
      font-size: 14px;
      font-weight: 600;
      display: none;
    }

    /* Live Telemetry Chips */
    .stats-row {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(130px, 1fr));
      gap: 12px;
    }
    .stat-chip {
      background: #0d1117;
      border: 1px solid var(--card-border);
      border-radius: 8px;
      padding: 10px;
      display: flex;
      flex-direction: column;
      gap: 4px;
    }
    .stat-chip .val {
      font-size: 16px;
      font-weight: 700;
      color: var(--cyan);
    }
    .stat-chip .lbl {
      font-size: 11px;
      color: var(--text-muted);
      text-transform: uppercase;
    }
  </style>
</head>
<body>
  <div class="container">
    <header>
      <div class="brand">
        <span class="logo-badge">AOOSTAR</span>
        <div>
          <h1>WTR Max LCD Manager</h1>
          <p class="hint">Front Panel Monitor & Power Controller</p>
        </div>
      </div>
      <div class="status-badge">
        <span class="pulse-dot"></span>
        <span id="header-status">Daemon Online</span>
      </div>
    </header>

    <!-- Live Preview Card -->
    <div class="preview-card">
      <div class="lcd-bezel">
        <img id="lcd-preview" src="/api/preview" alt="Live Front LCD Panel Preview" onerror="this.src='data:image/svg+xml;utf8,<svg xmlns=\'http://www.w3.org/2000/svg\' width=\'960\' height=\'376\' viewBox=\'0 0 960 376\'><rect width=\'960\' height=\'376\' fill=\'%230b0f19\'/><text x=\'50%25\' y=\'50%25\' font-family=\'sans-serif\' font-size=\'28\' fill=\'%238b949e\' dominant-baseline=\'middle\' text-anchor=\'middle\'>Display Backlight Asleep or Initializing...</text></svg>'">
      </div>
      <div class="preview-toolbar">
        <button onclick="setPower('on')" class="btn-primary">Power On</button>
        <button onclick="setPower('off')" class="btn-amber">Power Off</button>
        <button onclick="setPower('auto')">Auto Mode</button>
        <button onclick="nextPanel()">Next Panel</button>
        <button onclick="refreshPreview()">Refresh Snapshot</button>
      </div>
    </div>

    <!-- Live Telemetry Quick View -->
    <div class="card">
      <div class="card-title">Live Server Telemetry</div>
      <div class="stats-row">
        <div class="stat-chip"><span class="lbl">LAN IP</span><span class="val" id="stat-ip">--</span></div>
        <div class="stat-chip"><span class="lbl">CPU Temp</span><span class="val" id="stat-cpu-temp">--</span></div>
        <div class="stat-chip"><span class="lbl">CPU Load</span><span class="val" id="stat-cpu-load">--</span></div>
        <div class="stat-chip"><span class="lbl">RAM Used</span><span class="val" id="stat-ram">--</span></div>
        <div class="stat-chip"><span class="lbl">Net Upload</span><span class="val" id="stat-up">--</span></div>
        <div class="stat-chip"><span class="lbl">Net Download</span><span class="val" id="stat-down">--</span></div>
        <div class="stat-chip"><span class="lbl">Array Used</span><span class="val" id="stat-array">--</span></div>
        <div class="stat-chip"><span class="lbl">Disks Status</span><span class="val" id="stat-standby">--</span></div>
      </div>
    </div>

    <!-- Settings Forms -->
    <form id="settings-form" onsubmit="saveSettings(event)">
      <div class="grid">
        <!-- Display & Rotation Card -->
        <div class="card">
          <div class="card-title">Panel & Rotation Settings</div>
          
          <div class="form-group">
            <label>Active Panels</label>
            <p class="hint">Choose which screens rotate on the front display:</p>
            <div class="checkbox-group">
              <label class="checkbox-item">
                <input type="checkbox" id="panel-1" value="1">
                <div><strong>Panel 1</strong>: System Overview (CPU, RAM, GPU, 10GbE Network)</div>
              </label>
              <label class="checkbox-item">
                <input type="checkbox" id="panel-2" value="2">
                <div><strong>Panel 2</strong>: Drive Temperatures (Storage, SSDs, HDDs with Standby)</div>
              </label>
              <label class="checkbox-item">
                <input type="checkbox" id="panel-3" value="3">
                <div><strong>Panel 3</strong>: Storage Capacities (Array Summary, SSD/HDD Usage)</div>
              </label>
            </div>
          </div>

          <div class="form-group">
            <div class="form-row">
              <label for="switch-time">Panel Rotation Interval</label>
              <span id="switch-time-val" style="color:var(--cyan); font-weight:700;">30s</span>
            </div>
            <input type="range" id="switch-time" min="5" max="120" step="5" value="30" oninput="document.getElementById('switch-time-val').textContent = this.value + 's'">
            <p class="hint">Seconds each active panel stays on screen before rotating.</p>
          </div>

          <div class="form-group">
            <label>Temperature Unit</label>
            <select id="temp-unit">
              <option value="C">Celsius (&deg;C)</option>
              <option value="F">Fahrenheit (&deg;F)</option>
            </select>
          </div>

          <div class="form-group">
            <label for="net-iface">Network Interface</label>
            <input type="text" id="net-iface" placeholder="auto">
            <p class="hint">Use "auto" to monitor highest active 10GbE port, or pin to eth0, eth1, br0, bond0.</p>
          </div>
        </div>

        <!-- Power Management Card -->
        <div class="card">
          <div class="card-title">Screen Power & Sleep Management</div>

          <div class="form-group">
            <div class="form-row">
              <label for="sleep-schedule">Night Mode / Sleep Schedule</label>
              <label class="toggle-switch">
                <input type="checkbox" id="sleep-schedule">
                <span class="slider"></span>
              </label>
            </div>
            <p class="hint">Turn off LCD backlight during overnight hours.</p>
            <div style="display:grid; grid-template-columns: 1fr 1fr; gap:12px; margin-top:8px;">
              <div>
                <label class="hint" for="sleep-start">Sleep At (HH:MM)</label>
                <input type="time" id="sleep-start" value="23:00">
              </div>
              <div>
                <label class="hint" for="sleep-end">Wake At (HH:MM)</label>
                <input type="time" id="sleep-end" value="07:00">
              </div>
            </div>
          </div>

          <div class="form-group" style="margin-top:12px;">
            <div class="form-row">
              <label for="sleep-standby">Sleep on HDD Spindown</label>
              <label class="toggle-switch">
                <input type="checkbox" id="sleep-standby">
                <span class="slider"></span>
              </label>
            </div>
            <p class="hint">Turn off LCD backlight when all array drives are in Standby.</p>
            <div class="form-row" style="margin-top:8px;">
              <span class="hint">Delay before sleeping:</span>
              <span id="standby-delay-val" style="color:var(--orange); font-weight:700;">5 min</span>
            </div>
            <input type="range" id="standby-delay" min="60" max="1800" step="60" value="300" oninput="document.getElementById('standby-delay-val').textContent = (this.value / 60) + ' min'">
          </div>

          <div class="form-group" style="margin-top:12px;">
            <label class="checkbox-item">
              <input type="checkbox" id="wake-disk" checked>
              <div><strong>Wake on Disk Activity</strong>: Instantly wake screen when any disk spins up</div>
            </label>
            <label class="checkbox-item" style="margin-top:8px;">
              <input type="checkbox" id="wake-net">
              <div><strong>Wake on Network Activity</strong>: Keep awake during heavy file transfers (&gt; 5MB/s)</div>
            </label>
          </div>
        </div>
      </div>

      <!-- Save Bar -->
      <div class="save-bar">
        <div>
          <strong>Ready to apply?</strong>
          <span class="hint" style="margin-left:8px;">Changes take effect live in &lt; 5s without restarting.</span>
        </div>
        <div style="display:flex; align-items:center; gap:16px;">
          <span class="toast" id="save-toast">Settings saved & applied live!</span>
          <button type="submit" class="btn-primary">Save Settings</button>
        </div>
      </div>
    </form>
  </div>

  <script>
    let currentSettings = {};

    async function loadStatus() {
      try {
        const res = await fetch('/api/status');
        if (!res.ok) return;
        const data = await res.json();
        
        if (data.sensors) {
          const s = data.sensors;
          document.getElementById('stat-ip').textContent = s.net_ip_address || s.net_default_ip_address || '--';
          document.getElementById('stat-cpu-temp').textContent = s.cpu_temperature ? s.cpu_temperature + '°' : '--';
          document.getElementById('stat-cpu-load').textContent = s.cpu_percent ? s.cpu_percent + '%' : '--';
          document.getElementById('stat-ram').textContent = s.memory_Temperature || '--';
          document.getElementById('stat-up').textContent = s.net_upload_speed || '--';
          document.getElementById('stat-down').textContent = s.net_download_speed || '--';
          document.getElementById('stat-array').textContent = s.unraid_array_summary || '--';
          
          if (s.unraid_all_disks_standby === 'true') {
            document.getElementById('stat-standby').textContent = 'All Standby';
            document.getElementById('stat-standby').style.color = '#ff9100';
          } else {
            const active = s.unraid_active_disks || '?';
            const total = s.unraid_total_disks || '?';
            document.getElementById('stat-standby').textContent = active + '/' + total + ' Active';
            document.getElementById('stat-standby').style.color = '#00e5ff';
          }
        }
      } catch (e) {
        console.error("Status load failed", e);
      }
    }

    async function loadSettings() {
      try {
        const res = await fetch('/api/settings');
        if (!res.ok) return;
        currentSettings = await res.json();
        
        // Populate form
        const g = currentSettings.general || {};
        const p = currentSettings.power || {};
        const n = currentSettings.network || {};

        document.getElementById('switch-time').value = g.switchTime || 30;
        document.getElementById('switch-time-val').textContent = (g.switchTime || 30) + 's';
        document.getElementById('temp-unit').value = g.tempUnit || 'C';
        document.getElementById('net-iface').value = n.interface || 'auto';

        const activePanels = g.activePanels || [1, 2, 3];
        document.getElementById('panel-1').checked = activePanels.includes(1);
        document.getElementById('panel-2').checked = activePanels.includes(2);
        document.getElementById('panel-3').checked = activePanels.includes(3);

        document.getElementById('sleep-schedule').checked = !!p.sleepScheduleEnabled;
        document.getElementById('sleep-start').value = p.sleepStartTime || '23:00';
        document.getElementById('sleep-end').value = p.sleepEndTime || '07:00';

        document.getElementById('sleep-standby').checked = !!p.sleepOnAllDisksStandby;
        document.getElementById('standby-delay').value = p.standbyDelaySeconds || 300;
        document.getElementById('standby-delay-val').textContent = ((p.standbyDelaySeconds || 300) / 60) + ' min';

        document.getElementById('wake-disk').checked = p.wakeOnDiskActivity !== false;
        document.getElementById('wake-net').checked = !!p.wakeOnNetworkActivity;
      } catch (e) {
        console.error("Settings load failed", e);
      }
    }

    async function saveSettings(e) {
      e.preventDefault();
      const activePanels = [];
      if (document.getElementById('panel-1').checked) activePanels.push(1);
      if (document.getElementById('panel-2').checked) activePanels.push(2);
      if (document.getElementById('panel-3').checked) activePanels.push(3);
      if (activePanels.length === 0) activePanels.push(1);

      const payload = {
        general: {
          webPort: currentSettings.general?.webPort || 8744,
          switchTime: parseInt(document.getElementById('switch-time').value, 10),
          refreshInterval: currentSettings.general?.refreshInterval || 1,
          tempUnit: document.getElementById('temp-unit').value,
          activePanels: activePanels
        },
        network: {
          interface: document.getElementById('net-iface').value.trim() || 'auto'
        },
        power: {
          sleepScheduleEnabled: document.getElementById('sleep-schedule').checked,
          sleepStartTime: document.getElementById('sleep-start').value,
          sleepEndTime: document.getElementById('sleep-end').value,
          sleepOnAllDisksStandby: document.getElementById('sleep-standby').checked,
          standbyDelaySeconds: parseInt(document.getElementById('standby-delay').value, 10),
          wakeOnDiskActivity: document.getElementById('wake-disk').checked,
          wakeOnNetworkActivity: document.getElementById('wake-net').checked,
          networkWakeThresholdBytes: currentSettings.power?.networkWakeThresholdBytes || 5242880
        },
        sensors: currentSettings.sensors || {
          sensorRefreshSeconds: 2,
          diskRefreshSeconds: 60,
          enableSmart: false
        }
      };

      try {
        const res = await fetch('/api/settings', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(payload)
        });
        if (res.ok) {
          const toast = document.getElementById('save-toast');
          toast.style.display = 'inline';
          setTimeout(() => { toast.style.display = 'none'; }, 4000);
        }
      } catch (err) {
        alert("Failed to save settings: " + err);
      }
    }

    async function setPower(action) {
      try {
        await fetch('/api/power', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ action: action })
        });
        setTimeout(refreshPreview, 1000);
      } catch (e) {
        console.error("Power action failed", e);
      }
    }

    async function nextPanel() {
      try {
        await fetch('/api/panel/switch', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ action: 'next' })
        });
        setTimeout(refreshPreview, 1200);
      } catch (e) {
        console.error("Next panel failed", e);
      }
    }

    function refreshPreview() {
      const img = document.getElementById('lcd-preview');
      img.src = '/api/preview?t=' + new Date().getTime();
    }

    setInterval(refreshPreview, 2000);
    setInterval(loadStatus, 3000);

    loadSettings();
    loadStatus();
  </script>
</body>
</html>
"#;