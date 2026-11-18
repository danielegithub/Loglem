'use strict';

// Stato sempre a vista: riquadri, fascia in alto, riga a sinistra, e i pannelli di «Sistema»
// per servizio, controllo automatico e avvisi per mail.

const MINUTE = 60000;

function minutesSince(timestamp) {
  const then = Date.parse(timestamp);
  return Number.isNaN(then) ? null : Math.max(0, Math.floor((Date.now() - then) / MINUTE));
}

// «adesso», «5 min fa», «3 h fa», «2 giorni fa».
function ago(timestamp) {
  const m = minutesSince(timestamp);
  if (m === null) return '—';
  if (m < 1) return 'adesso';
  if (m < 60) return m + ' min fa';
  if (m < 48 * 60) return Math.floor(m / 60) + ' h fa';
  return Math.floor(m / 1440) + ' giorni fa';
}

// «da 5 minuti», «da 3 ore», «da 2 giorni».
function lasting(timestamp) {
  const m = minutesSince(timestamp);
  if (m === null) return '—';
  if (m < 1) return 'appena avviato';
  if (m < 60) return m + (m === 1 ? ' minuto' : ' minuti');
  if (m < 48 * 60) return Math.floor(m / 60) + ' ore';
  return Math.floor(m / 1440) + ' giorni';
}

function clock(timestamp) {
  const date = new Date(timestamp);
  return Number.isNaN(date.valueOf())
    ? '—'
    : date.toLocaleTimeString('it-IT', { hour: '2-digit', minute: '2-digit' });
}

function dayAndClock(timestamp) {
  const date = new Date(timestamp);
  return Number.isNaN(date.valueOf())
    ? '—'
    : date.toLocaleString('it-IT', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' });
}

function setCard(id, text, level) {
  const strong = $(id);
  if (!strong) return;
  strong.textContent = text;
  strong.parentElement.classList.remove('bad', 'warn');
  if (level) strong.parentElement.classList.add(level);
}

// Dice se l'ultimo invio di mail non è riuscito: un avviso che non arriva è un problema.
function emailWarning() {
  const mail = state.live?.email;
  if (!mail?.active || !mail.last || mail.last.ok) return null;
  return 'Avvisi per mail: l’ultimo invio non è riuscito' + (mail.pending ? ' (riprovo da solo)' : '') +
    '. Controlla in Sistema → Avvisi per mail.';
}

function renderLive() {
  const live = state.live;
  if (!live) return;
  const finished = state.collection?.last_finished;
  $('last-reading').textContent = 'Ultima lettura: ' + (finished ? ago(finished) : '—');

  const recovered = (state.sources || []).some((s) => s.recovery);
  // Rosso se Loglem è stato fermo di recente (le sorgenti mostrano «Ripresa dopo X di fermo»).
  setCard('uptime', lasting(live.started_at), recovered ? 'bad' : '');
  $('card-uptime').querySelector('span').textContent = 'Loglem acceso da';

  const m = live.monitor;
  const text = { off: 'spento', never: 'mai' }[m.state] || ago(m.checked_at);
  setCard('last-check', text, m.state === 'stale' ? 'bad' : m.state === 'off' || m.state === 'never' ? 'warn' : '');

  const open = (state.problems || []).length;
  setCard('open-alerts', String(open), open > 0 ? 'bad' : '');

  if (open === 0) renderProblems([]);
}

// La fascia in alto quando non ci sono problemi: dice che tutto va bene, o perché non ci si può fidare.
function calmStatus(box) {
  const m = state.live?.monitor;
  const mail = emailWarning();
  box.replaceChildren();
  if (!m) {
    box.hidden = true;
    return;
  }
  let level = 'ok';
  let text;
  if (m.state === 'off') {
    level = 'stale';
    text = 'Il controllo automatico è spento: nessuno controlla per te. ';
  } else if (m.state === 'stale') {
    level = 'stale';
    text = 'Il controllo non gira da ' + m.age_minutes + ' minuti (dovrebbe girare ogni ' + m.minutes + '). ';
  } else if (m.state === 'never') {
    text = 'Sto per fare il primo controllo…';
  } else {
    text = 'Tutto a posto, controllato alle ' + clock(m.checked_at) + '.';
  }
  if (mail && level === 'ok') level = 'stale';
  box.className = 'problems ' + level;
  box.append(element('span', text));
  if (level === 'stale' && (m.state === 'off' || m.state === 'stale')) {
    box.append(link('Apri Sistema', '#system', 'link-button'));
  }
  if (mail) box.append(element('div', mail));
  box.hidden = false;
}

// ---------- pannelli di «Sistema» ----------

let serviceInfo = null;

function renderService(info) {
  serviceInfo = info;
  const badge = $('service-state');
  const detail = $('service-detail');
  const install = $('service-install');
  const restart = $('service-restart');
  badge.className = 'service-state';
  install.disabled = !info.supported;
  restart.disabled = !info.supported || !info.installed;
  if (!info.supported) {
    badge.textContent = 'Non disponibile su questo sistema';
    detail.textContent = 'Il servizio automatico c’è su Linux (systemd) e macOS (launchd). Tieni aperto «loglem web».';
    return;
  }
  if (info.failed) {
    badge.classList.add('bad');
    badge.textContent = '● Fermo, serve un intervento';
    detail.textContent = 'Il servizio si è fermato con un errore o ha finito i riavvii automatici (10 in 5 minuti). ' +
      'Guarda «Problemi», correggi la causa e premi Riavvia.';
  } else if (info.active) {
    badge.classList.add('ok');
    badge.textContent = '● Attivo' + (info.since ? ' da ' + lasting(info.since) : '') +
      (info.restarts != null ? ' · Riavvii: ' + info.restarts : '');
    detail.textContent = info.this_process
      ? 'Questa pagina è servita dal servizio.'
      : 'Il servizio è acceso, ma questa pagina viene da un altro Loglem (avviato a mano con «loglem web»).';
  } else if (info.installed) {
    badge.classList.add('warn');
    badge.textContent = '● Installato ma fermo';
    detail.textContent = info.this_process
      ? ''
      : 'Questa pagina viene da un Loglem avviato a mano. Per usare il servizio chiudilo e lancia «loglem service start».';
  } else {
    badge.classList.add('warn');
    badge.textContent = '● Non installato';
    detail.textContent = 'Loglem legge i log solo finché tieni aperto questo programma. ' +
      'Installa il servizio per tenerlo sempre acceso: parte da solo e riparte dopo un guasto.';
  }
  if (info.file) detail.textContent += (detail.textContent ? ' ' : '') + 'File: ' + info.file;
}

async function loadService() {
  try {
    renderService(await api('service'));
  } catch (error) {
    $('service-state').textContent = 'Stato del servizio non disponibile';
    $('service-detail').textContent = error.message;
  }
}

$('service-install').addEventListener('click', () =>
  perform($('service-install'), async () => {
    const question = serviceInfo?.installed
      ? 'Reinstallare il servizio? Il file di servizio viene riscritto con questo programma e questa cartella dati.'
      : 'Installare il servizio? Loglem scrive un file di servizio nella tua cartella utente e lo abilita: ' +
        'partirà da solo e il controllo automatico girerà al suo interno. Lo puoi togliere con «loglem service uninstall».';
    if (!confirm(question)) return;
    const result = await api('service-install');
    renderService(result.service);
    showNotice(result.message);
  }),
);

$('service-restart').addEventListener('click', () =>
  perform($('service-restart'), async () => {
    if (!confirm('Riavviare Loglem? Per qualche secondo la pagina non risponde, poi si ricarica da sola.')) return;
    await api('service-restart');
    const text = $('service-state');
    text.className = 'service-state warn';
    text.textContent = '● Riavvio in corso…';
    let wentDown = false;
    for (let i = 0; i < 60; i++) {
      await sleep(1000);
      try {
        const reply = await fetch(location.pathname, { cache: 'no-store' });
        if (reply.ok && wentDown) {
          location.reload();
          return;
        }
      } catch (_) {
        wentDown = true;
        text.textContent = '● Riavvio in corso… (Loglem si sta riaccendendo)';
      }
    }
    text.className = 'service-state bad';
    text.textContent = wentDown
      ? '● Non è ripartito entro un minuto. Guarda lo stato qui sotto o il terminale.'
      : '● Il riavvio non risulta eseguito. Guarda il terminale.';
    loadService();
  }),
);

function fillMonitor() {
  const m = state.live?.monitor;
  if (!m) return;
  $('monitor-on').checked = m.enabled;
  $('monitor-minutes').value = m.minutes;
  showMonitorResult();
}

function showMonitorResult() {
  const m = state.live?.monitor;
  if (!m) return;
  $('monitor-result').textContent = m.state === 'off'
    ? 'Spento: nessun controllo automatico.'
    : m.checked_at
      ? 'Ultimo controllo: ' + dayAndClock(m.checked_at) + ' · ' +
        (m.open ? m.open + (m.open === 1 ? ' avviso aperto' : ' avvisi aperti') : 'tutto a posto')
      : 'Non è ancora stato eseguito.';
}

submitForm('monitor-form', async () => {
  const result = await api('monitor-save', {
    enabled: $('monitor-on').checked,
    minutes: Number($('monitor-minutes').value),
  });
  state.live = { ...state.live, monitor: result.monitor };
  renderLive();
  showMonitorResult();
  showNotice(result.message);
});

$('monitor-now').addEventListener('click', () =>
  perform($('monitor-now'), async () => {
    const result = await api('monitor-check');
    state.live = { ...state.live, monitor: result.monitor };
    await refresh();
    showMonitorResult();
    showNotice(result.message);
  }),
);

function showEmail(info) {
  $('email-from').value = info.from || '';
  $('email-to').value = (info.to || []).join(', ');
  $('email-page').value = info.page_url || '';
  $('email-limit').value = info.hourly_limit || 6;
  $('email-key').value = '';
  $('email-key-note').textContent = info.key_set
    ? 'Una chiave è già salvata e non si può rileggere. Lasciala vuota per tenerla, o incollane una nuova per cambiarla.'
    : 'Nessuna chiave salvata. In Resend crea una chiave di sola spedizione (Sending access) limitata al tuo dominio.';
  const last = info.last;
  $('email-last').textContent = last
    ? 'Ultimo invio: ' + dayAndClock(last.at) + ' · ' + (last.ok ? 'riuscito' : 'NON riuscito: ' + (last.error || '')) +
      (info.pending ? ' · una mail aspetta di essere rimandata' : '')
    : info.configured ? 'Nessun invio ancora. Premi «Invia una mail di prova».' : '';
  $('email-test').disabled = !info.active;
  $('email-off').disabled = !info.configured;
}

async function loadEmail() {
  try {
    showEmail(await api('email-status'));
  } catch (error) {
    $('email-last').textContent = error.message;
  }
}

submitForm('email-form', async () => {
  const result = await api('email-save', {
    key: $('email-key').value,
    from: $('email-from').value,
    to: $('email-to').value,
    page_url: $('email-page').value,
    hourly_limit: Number($('email-limit').value),
  });
  showEmail(result.email);
  showNotice(result.message);
});

$('email-test').addEventListener('click', () =>
  perform($('email-test'), async () => {
    try {
      const result = await api('email-test');
      showEmail(result.email);
      showNotice(result.message);
    } finally {
      loadEmail();
    }
  }),
);

$('email-off').addEventListener('click', () =>
  perform($('email-off'), async () => {
    if (!confirm('Spegnere gli avvisi per mail e cancellare la chiave di Resend da questo computer?')) return;
    const result = await api('email-off');
    showEmail(result.email);
    showNotice(result.message);
  }),
);

// Chiamata quando si apre «Sistema».
function loadSystemPanels() {
  fillMonitor();
  loadService();
  loadEmail();
}

// «5 min fa» invecchia anche quando non arrivano dati nuovi.
setInterval(() => {
  if (state.live) renderLive();
}, 20000);
