(function () {
  // ─── Frontend Debug Logger ───
  var FL = {
    entries: [],
    max: 200,
    enabled: true,
    log: function (tag, msg) {
      if (!this.enabled) return;
      var now = new Date();
      var time = now.toTimeString().slice(0, 8) + '.' + String(now.getMilliseconds()).padStart(3, '0');
      var level = 'log';
      this.entries.push({
        time: time,
        tag: tag,
        msg: msg,
        level: level
      });
      this.flush();
      this.consoleLog(level, tag, msg);
    },
    info: function (tag, msg) {
      var now = new Date();
      var time = now.toTimeString().slice(0, 8) + '.' + String(now.getMilliseconds()).padStart(3, '0');
      this.entries.push({
        time: time,
        tag: tag,
        msg: msg,
        level: 'info'
      });
      this.flush();
      this.consoleLog('info', tag, msg);
    },
    warn: function (tag, msg) {
      var now = new Date();
      var time = now.toTimeString().slice(0, 8) + '.' + String(now.getMilliseconds()).padStart(3, '0');
      this.entries.push({
        time: time,
        tag: tag,
        msg: msg,
        level: 'warn'
      });
      this.flush();
      this.consoleLog('warn', tag, msg);
    },
    error: function (tag, msg) {
      var now = new Date();
      var time = now.toTimeString().slice(0, 8) + '.' + String(now.getMilliseconds()).padStart(3, '0');
      this.entries.push({
        time: time,
        tag: tag,
        msg: msg,
        level: 'error'
      });
      this.flush();
      this.consoleLog('error', tag, msg);
    },
    success: function (tag, msg) {
      var now = new Date();
      var time = now.toTimeString().slice(0, 8) + '.' + String(now.getMilliseconds()).padStart(3, '0');
      this.entries.push({
        time: time,
        tag: tag,
        msg: msg,
        level: 'success'
      });
      this.flush();
      this.consoleLog('success', tag, msg);
    },
    flush: function () {
      if (this.entries.length > this.max) {
        this.entries = this.entries.slice(-this.max);
      }
      this.render();
    },
    clear: function () {
      this.entries = [];
      this.render();
    },
    render: function () {
      var body = document.getElementById('debugBody');
      var count = document.getElementById('debugCount');
      if (!body) return;
      count.textContent = this.entries.length;
      var html = '';
      for (var i = 0; i < this.entries.length; i++) {
        var e = this.entries[i];
        html += '<div class="debug-log-entry ' + e.level + '">' + '<span class="debug-log-time">' + e.time + '</span>' + '<span class="debug-log-tag">[' + e.tag + ']</span>' + e.msg + '</div>';
      }
      body.innerHTML = html;
      body.scrollTop = body.scrollHeight;
    },
    consoleLog: function (level, tag, msg) {
      if (level === 'log') console.log('[' + tag + '] ' + msg);else if (level === 'info') console.info('[' + tag + '] ' + msg);else if (level === 'warn') console.warn('[' + tag + '] ' + msg);else if (level === 'error') console.error('[' + tag + '] ' + msg);else if (level === 'success') console.log('[' + tag + '] ' + msg);
    }
  };

  // Debug panel controls
  var panel = document.getElementById('debugPanel');
  var header = document.getElementById('debugHeader');
  var collapseBtn = document.getElementById('debugCollapse');
  var clearBtn = document.getElementById('debugClear');
  var fab = document.getElementById('debugToggle');
  if (header) {
    header.addEventListener('click', function () {
      panel.classList.toggle('collapsed');
      if (panel.classList.contains('collapsed')) {
        fab.classList.remove('hidden');
        if (collapseBtn) collapseBtn.textContent = '[ + ]';
      } else {
        fab.classList.add('hidden');
        if (collapseBtn) collapseBtn.textContent = '[ - ]';
      }
    });
  }
  if (collapseBtn) {
    collapseBtn.addEventListener('click', function (e) {
      e.stopPropagation();
      header.click();
    });
  }
  if (clearBtn) {
    clearBtn.addEventListener('click', function (e) {
      e.stopPropagation();
      FL.clear();
    });
  }
  if (fab) {
    fab.addEventListener('click', function () {
      panel.classList.remove('collapsed');
      fab.classList.add('hidden');
    });
  }
  FL.info('INIT', 'Dashboard started');

  // Read server-provided data from window.__HARetro__
  var d = window.__HARetro__;
  var initialLabels = d.chartLabels;
  var initialValues = d.chartValues;
  var bufferLabels = d.bufferChart.labels;
  var bufferTopVals = d.bufferChart.bufferTop;
  var bufferBottomVals = d.bufferChart.bufferBottom;
  var solarFlowVals = d.bufferChart.solarFlow;
  var solarReturnVals = d.bufferChart.solarReturn;
  var pumpBarData = d.pumpStates;
  var pumpBufferLabels = d.bufferChart.labels;
  var historyMinutes = d.historyMinutes;
  var maxDataPoints = 1080;
  var ctx = document.getElementById('solarChart');
  if (!ctx || !window.Chart) {
    console.error('Chart.js not loaded or canvas not found');
    return;
  }
  FL.info('CHART', 'Solar chart init — ' + initialLabels.length + ' data points');
  var gradient = ctx.getContext('2d').createLinearGradient(0, 0, 0, 260);
  gradient.addColorStop(0, 'rgba(100, 225, 255, 0.35)');
  gradient.addColorStop(1, 'rgba(100, 225, 255, 0.04)');
  while (initialLabels.length > maxDataPoints) {
    var step = Math.ceil(initialLabels.length / maxDataPoints);
    var newLabels = [];
    var newValues = [];
    for (var i = 0; i < initialLabels.length; i += step) {
      newLabels.push(initialLabels[i]);
      newValues.push(initialValues[i]);
    }
    initialLabels = newLabels;
    initialValues = newValues;
  }
  var chart = new Chart(ctx, {
    type: 'line',
    data: {
      labels: initialLabels.slice(-maxDataPoints),
      datasets: [{
        data: initialValues.slice(-maxDataPoints),
        borderColor: 'rgba(110, 255, 204, 0.95)',
        backgroundColor: gradient,
        borderWidth: 2,
        pointRadius: 0,
        lineTension: 0.2
      }]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      animation: {
        duration: 0
      },
      legend: {
        display: false
      },
      tooltips: {
        intersect: false,
        mode: 'index',
        displayColors: false,
        callbacks: {
          label: function (tooltipItem) {
            return tooltipItem.yLabel + ' W';
          }
        }
      },
      scales: {
        xAxes: [{
          gridLines: {
            color: 'rgba(255,255,255,0.05)'
          },
          ticks: {
            fontColor: '#8d96ab'
          }
        }],
        yAxes: [{
          gridLines: {
            color: 'rgba(255,255,255,0.05)'
          },
          ticks: {
            beginAtZero: true,
            fontColor: '#8d96ab'
          }
        }]
      }
    }
  });

  // Buffer temp chart
  var bufferCtx = document.getElementById('bufferChart');
  if (!bufferCtx) {
    console.error('Buffer canvas not found');
    return;
  }
  var bufferContext = bufferCtx.getContext('2d');
  var bufferGradient = bufferContext.createLinearGradient(0, 0, 0, 260);
  bufferGradient.addColorStop(0, 'rgba(100, 225, 255, 0.35)');
  bufferGradient.addColorStop(1, 'rgba(100, 225, 255, 0.04)');
  var bufferChart = new Chart(bufferContext, {
    type: 'line',
    data: {
      labels: bufferLabels.slice(-maxDataPoints),
      datasets: [{
        label: 'Puffer Oben',
        data: bufferTopVals.slice(-maxDataPoints),
        borderColor: '#F4BD4A',
        // orange
        borderWidth: 1.5,
        pointRadius: 0,
        lineTension: 0.1
      }, {
        label: 'Puffer Unten',
        data: bufferBottomVals.slice(-maxDataPoints),
        borderColor: '#4269D0',
        // blue
        borderWidth: 1.5,
        pointRadius: 0,
        lineTension: 0.1
      }, {
        label: 'Solar Vorlauf',
        data: solarFlowVals.slice(-maxDataPoints),
        borderColor: '#6CC5B0',
        // green
        borderWidth: 1.5,
        pointRadius: 0,
        lineTension: 0.1
      }, {
        label: 'Solar Ruecklauf',
        data: solarReturnVals.slice(-maxDataPoints),
        borderColor: '#FF725C',
        // red
        borderWidth: 1.5,
        pointRadius: 0,
        lineTension: 0.1
      }]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      animation: {
        duration: 0
      },
      legend: {
        display: true,
        labels: {
          fontColor: '#8d96ab',
          boxWidth: 12
        }
      },
      tooltips: {
        intersect: false,
        mode: 'index',
        displayColors: false
      },
      scales: {
        xAxes: [{
          gridLines: {
            color: 'rgba(255,255,255,0.05)'
          },
          ticks: {
            fontColor: '#8d96ab',
            maxTicksLimit: 8
          }
        }],
        yAxes: [{
          gridLines: {
            color: 'rgba(255,255,255,0.05)'
          },
          ticks: {
            fontColor: '#8d96ab'
          }
        }]
      }
    }
  });
  console.log('Buffer chart initialized');

  // Pump status bar
  FL.info('PUMP', 'pumpBarData length=' + pumpBarData.length + ', labels=' + pumpBufferLabels.length);
  if (pumpBarData.length > 0) {
    FL.info('PUMP', 'Rendering pump bar on init with ' + pumpBarData.length + ' samples');
    renderPumpBar(pumpBarData, pumpBufferLabels);
  } else {
    FL.info('PUMP', 'Skipping render — no pump states');
  }
  console.log('Pump status bar initialized with ' + pumpBarData.length + ' samples');

  // Init buffer top tile
  var bufferTopEl = document.getElementById('buffer-top-value');
  if (bufferTopEl && bufferTopVals.length > 0) {
    var lastTop = bufferTopVals[bufferTopVals.length - 1];
    if (lastTop != null) bufferTopEl.textContent = parseFloat(lastTop).toFixed(1) + '\u00b0C';
  }
  function renderPumpBar(states, labels) {
    var canvas = document.getElementById('pumpStatusChart');
    if (!canvas) {
      FL.warn('PUMP', 'canvas not found');
      return;
    }
    var ctx = canvas.getContext('2d');
    var dpr = window.devicePixelRatio || 1;
    var rect = canvas.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return;
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);
    var w = rect.width;
    var h = rect.height;
    var now = Date.now();
    var historyMs = historyMinutes * 60 * 1000;
    var windowStart = now - historyMs;
    var windowEnd = now;
    FL.info('PUMP', 'windowStart=' + windowStart + ' windowEnd=' + windowEnd + ' statesLen=' + states.length + ' labelsLen=' + labels.length);
    if (states.length > 0) {
      FL.info('PUMP', 'sample state t=' + states[0].t + ' ts=' + new Date(states[0].t).toISOString());
    }

    // Draw background (red = off)
    ctx.fillStyle = 'rgba(220, 80, 80, 0.55)';
    ctx.fillRect(0, 0, w, h);

    // Overlay green segments where pump was on
    var greenSegments = 0;
    for (var i = 0; i < states.length; i++) {
      var s = states[i];
      var segStart = Math.max(s.t, windowStart);
      var x1 = (segStart - windowStart) / (windowEnd - windowStart) * w;

      // Find end of consecutive same-state run
      var j = i;
      var x2 = x1;
      while (j < states.length && states[j].on === s.on) {
        var next = states[j];
        var nextStart = Math.max(next.t, windowStart);
        var nextX = (nextStart - windowStart) / (windowEnd - windowStart) * w;
        if (j < states.length - 1) {
          var nx2 = (Math.max(states[j + 1].t, windowStart) - windowStart) / (windowEnd - windowStart) * w;
          nextX = nx2;
        }
        x2 = Math.max(x2, nextX);
        j++;
      }
      if (s.on) {
        ctx.fillStyle = 'rgba(50, 200, 100, 0.7)';
        ctx.fillRect(x1, 0, Math.max(x2 - x1, 1), h);
        greenSegments++;
      }
      i = j - 1;
    }
    FL.info('PUMP', 'drawn ' + greenSegments + ' green segments out of ' + states.length + ' total');

    // Add subtle border
    ctx.strokeStyle = 'rgba(255,255,255,0.15)';
    ctx.lineWidth = 1;
    ctx.strokeRect(0, 0, w, h);
  }

  // Resize handler for pump bar
  window.addEventListener('resize', function () {
    if (pumpBarData.length > 0) {
      renderPumpBar(pumpBarData, pumpBufferLabels);
    }
  });
  console.log('Pump status bar initialized');
  console.log('Solar WebSocket started');
  var socket = null;
  var pendingMessages = [];
  function updateSolarData(data) {
    console.log('Solar data updated:', data.watts, 'W');
    FL.info('SYNC', 'solar: ' + Math.round(data.watts) + 'W | charger: ' + (data.charger_amps != null ? data.charger_amps : '-') + 'A | ' + (data.buffer_temps ? 'buffer_temps' : 'no buffer') + ' | ' + (data.pump_status ? 'pump' : 'no pump'));

    // Update current output stat
    var wattsEl = document.querySelector('.stat:first-child .stat-value');
    if (wattsEl) wattsEl.textContent = Math.round(data.watts) + ' W';

    // Update chart
    if (data.chart_labels && data.chart_values) {
      chart.data.labels = data.chart_labels.slice(-maxDataPoints);
      chart.data.datasets[0].data = data.chart_values.slice(-maxDataPoints);
      chart.update();
    }

    // Update charger section
    if (data.charger_amps !== undefined) {
      var ampsEl = document.querySelector('.charger-reading');
      if (ampsEl) ampsEl.textContent = data.charger_amps;
    }
    if (data.charger_status !== undefined) {
      var statusEl = document.querySelector('.charger-status');
      if (statusEl) statusEl.textContent = data.charger_status;
    }
    if (data.charger_car_state !== undefined) {
      var carEl = document.querySelector('.charger-car-state');
      if (carEl) {
        carEl.textContent = data.charger_car_state;
        carEl.className = 'charger-car-state ' + data.charger_car_state_class;
      }
    }
    if (data.charger_car_connected !== undefined) {
      var pillEl = document.querySelector('.charger-pill');
      if (pillEl) {
        if (data.charger_paused) {
          pillEl.textContent = 'PAUSIERT';
          pillEl.className = 'charger-pill charger-pill-paused';
        } else if (data.charger_charging) {
          pillEl.textContent = 'Am Laden';
          pillEl.className = 'charger-pill charger-pill-charging';
        } else if (data.charger_car_connected) {
          pillEl.textContent = 'Anschluss';
          pillEl.className = 'charger-pill';
        } else {
          pillEl.textContent = 'Nicht angeschlossen';
          pillEl.className = 'charger-pill charger-pill-disconnected';
        }
      }
    }
    if (data.garage_left) {
      updateGarage(data.garage_left, 'left');
    }
    if (data.garage_right) {
      updateGarage(data.garage_right, 'right');
    }

    // Update buffer temp chart
    if (data.buffer_temps) {
      if (bufferChart && data.buffer_temps.labels) {
        bufferChart.data.labels = data.buffer_temps.labels.slice(-maxDataPoints);
        bufferChart.data.datasets[0].data = (data.buffer_temps.buffer_top || []).slice(-maxDataPoints);
        bufferChart.data.datasets[1].data = (data.buffer_temps.buffer_bottom || []).slice(-maxDataPoints);
        bufferChart.data.datasets[2].data = (data.buffer_temps.solar_flow || []).slice(-maxDataPoints);
        bufferChart.data.datasets[3].data = (data.buffer_temps.solar_return || []).slice(-maxDataPoints);
        bufferChart.update();
      }
    }

    // Update pump status bar
    if (data.pump_states && data.pump_states.length > 0) {
      renderPumpBar(data.pump_states, pumpBufferLabels);
    }

    // Update pump tile
    if (data.pump_status) {
      var tileEl = document.getElementById('pump-tile');
      if (tileEl) {
        tileEl.className = 'pump-tile ' + data.pump_status.css_class;
      }
      var pumpStateEl = document.getElementById('pump-state');
      if (pumpStateEl) pumpStateEl.textContent = data.pump_status.status_label;
      var pumpInfoEl = document.getElementById('pump-info');
      if (pumpInfoEl) {
        if (data.pump_status.pump_on) {
          pumpInfoEl.innerHTML = data.pump_status.is_correct ? '\u2705 Heat flowing into buffer' : '\u274c No heat transfer';
        } else {
          pumpInfoEl.innerHTML = data.pump_status.is_correct ? '\u2705 Normal operation' : '\u274c Should be running';
        }
      }
    }

    // Update last updated timestamp
    var lastUpdatedEl = document.querySelector('.last-updated');
    if (lastUpdatedEl) {
      lastUpdatedEl.textContent = 'Letztes Update: ' + new Date(Date.now()).toLocaleString("de-DE");
    }
  }
  function updateGarage(data, side) {
    var wrapper = document.getElementById('garage-' + side + '-wrapper');
    if (!wrapper) {
      FL.warn('GARAGE', side + ' wrapper not found');
      return;
    }
    var statusEl = wrapper.querySelector('.status');
    var btnEl = wrapper.querySelector('button');
    FL.info('GARAGE', side + ': ' + data.status + ' \u2192 ' + data.action);
    if (statusEl) statusEl.textContent = data.status;
    if (btnEl) {
      btnEl.textContent = data.action;
      btnEl.setAttribute('class', data.button_class);
    }
  }
  function wsSend(cmd) {
    var msg = JSON.stringify(cmd);
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      FL.warn('WS', 'send deferred (not open), pending=' + pendingMessages.length + ' \u2192 ' + cmd.action + ' ' + cmd.entity_id);
      pendingMessages.push(msg);
      return;
    }
    socket.send(msg);
    FL.info('WS', 'send \u2192 ' + cmd.action + ' entity=' + (cmd.entity_id || '-'));
  }
  function connect() {
    FL.info('WS', 'connecting \u2192 ws://' + location.host + '/ws/solar');
    socket = new WebSocket('ws://' + location.host + '/ws/solar');
    socket.onopen = function () {
      FL.success('WS', 'connected \u2014 flushed ' + pendingMessages.length + ' pending messages');
      for (var i = 0; i < pendingMessages.length; i++) {
        socket.send(pendingMessages[i]);
      }
      pendingMessages = [];
    };
    socket.onmessage = function (event) {
      try {
        var data = JSON.parse(event.data);
        updateSolarData(data);
      } catch (e) {
        FL.error('WS', 'parse failed: ' + e.message + ' | data=' + event.data.substring(0, 200));
      }
    };
    socket.onclose = function () {
      FL.warn('WS', 'closed \u2014 reconnecting on next visible event');
      socket = null;
    };
  }
  document.addEventListener('visibilitychange', function () {
    if (document.visibilityState === 'hidden' && socket) {
      FL.info('WS', 'page hidden \u2014 closing socket');
      socket.close();
    }
  });
  document.addEventListener('visibilitychange', function () {
    if (document.visibilityState === 'visible') {
      FL.info('VIS', 'page visible \u2014 opening WebSocket');
      connect();
    }
  });
  connect();

  // Attach wsSend click handlers programmatically (inline onclick can't see wsSend inside IIFE)
  var refreshBtn = document.querySelector('.refresh-btn');
  if (refreshBtn) {
    refreshBtn.addEventListener('click', function (e) {
      e.preventDefault();
      FL.info('CLICK', 'refresh btn clicked');
      wsSend({
        action: 'force_refresh'
      });
    });
  }
  document.querySelectorAll('.garage-card button').forEach(function (el) {
    el.addEventListener('click', function () {
      FL.info('CLICK', 'garage toggle \u2192 entity=' + el.dataset.entityId);
      wsSend({
        action: 'toggle',
        entity_id: el.dataset.entityId
      });
    });
  });
  document.querySelectorAll('.toggle-btn, .script-btn').forEach(function (btn) {
    var entity = btn.getAttribute('data-entity-id');
    if (entity) {
      var action = btn.classList.contains('script-btn') ? 'run_script' : 'toggle';
      btn.addEventListener('click', function () {
        FL.info('CLICK', action + ' \u2192 entity=' + entity);
        wsSend({
          action: action,
          entity_id: entity
        });
      });
    }
  });
  FL.success('HANDLERS', 'all click handlers attached: refresh, garage, toggle, scripts');
})();