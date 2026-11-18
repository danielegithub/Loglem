'use strict';

const $ = (id) => document.getElementById(id);
const token = document.querySelector('meta[name="loglem-token"]').content;

const REFRESH_MS = 2000;
const PAGES = {
  rules: ['Regole e siti', 'Gestisci le classi, le quote degli archivi e le assegnazioni.'],
  inventory: ['Mappa del server', 'Sorgenti visibili su questo server e collegamenti a Loglem.'],
  sources: ['Siti e server', 'Ogni scheda raccoglie i log di un sito o del sistema.'],
  site: ['Log del gruppo', ''],
  'new-site': ['Nuovo sito', 'Potrai collegare i log dopo aver scelto il nome.'],
  logs: ['Esplora i log', 'Scegli una sorgente, cerca una parola e filtra per data.'],
  add: ['Aggiungi un log', 'Cosa, chi, dove: ti guido un passo alla volta.'],
  settings: ['Impostazioni sorgente', 'Nome, durata, posizione dei dati e collegamento.'],
  diagnostics: ['Problemi', 'Se qualcosa non funziona, qui trovi cosa è successo.'],
  privacy: ['Dati personali', 'Cancella o scarica i dati di una persona, e guarda chi ha consultato i log.'],
  system: ['Sistema', 'Aggiornamenti e impostazioni da spostare su un altro server.'],
  stats: ['Statistiche', 'Richieste, pagine, codici di risposta e indirizzi di un log di accesso.'],
};

let state = { sources: [], sites: {}, collection: {} };
let view = 'sources';
let settingsSource = null;
let statusRequest = null;
let searching = false;
let lastSignature = '';
let searchSequence = 0;

// ---------- utilità ----------

function element(tag, text, className) {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
}

function link(label, href, className = 'button') {
  const a = element('a', label, className);
  a.href = href;
  return a;
}

function showNotice(message, isError = false) {
  const box = $('notice');
  box.textContent = message;
  box.className = isError ? 'error' : '';
  box.hidden = false;
}

function goToSources(message) {
  if (location.hash === '#sources') showNotice(message);
  else {
    window.addEventListener('hashchange', () => showNotice(message), { once:true });
    location.hash = '#sources';
  }
}

function hideNotice() {
  $('notice').hidden = true;
}

let sessionLocked = false;
const sessionRetryKey = 'loglem-session-retry';
async function recoverSession(restarted = false) {
  if (sessionLocked) return;
  sessionLocked = true;
  document.body.classList.add('session-locked');
  for (const panel of document.querySelectorAll('main, aside, dialog[open]')) {
    if (panel.tagName === 'DIALOG') panel.close();
    panel.inert = true;
    for (const control of panel.querySelectorAll('button, input, select, textarea')) control.disabled = true;
  }
  const dialog = $('session-block');
  $('session-message').textContent = restarted ? 'Loglem ha riavviato la sessione. Ricarico questa pagina…' : 'La sessione è cambiata. Sto ricollegando la pagina…';
  dialog.showModal();
  try {
    if (!sessionStorage.getItem(sessionRetryKey)) {
      sessionStorage.setItem(sessionRetryKey, '1');
      const page = await fetch(location.href, { cache: 'no-store', signal: AbortSignal.timeout(5000) });
      if (page.ok) {
        location.reload();
        return;
      }
    }
  } catch (_) {}
  $('session-message').textContent = 'La connessione alla sessione non è stata ripristinata. Ricarica per continuare. Le modifiche in corso non sono state salvate.';
}
$('session-block').addEventListener('cancel', event => event.preventDefault());
$('retry-session').onclick = () => location.reload();

async function api(path, data = {}) {
  if (sessionLocked) throw new Error("Ricarica la pagina per continuare.");
  let response;
  try {
    response = await fetch('api/' + path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Loglem-Token': token },
      body: JSON.stringify(data),
    });
  } catch (_) {
    throw new Error('Loglem non risponde. Controlla che sia ancora aperto nel terminale.');
  }
  const type = response.headers.get('content-type') || '';
  const result = type.includes('application/json') ? await response.json() : null;
  if (response.status === 401 || result?.session_expired ||
      (response.status === 403 && !result && /session.*required/i.test(await response.text()))) {
    await recoverSession(result?.local_session === true);
    throw new Error('Ricollegamento della sessione in corso.');
  }
  if (!result && !type.includes('application/json')) throw new Error('Risposta inattesa di Loglem. Riprova tra poco.');
  if (path === 'status' && response.ok) {
    try { sessionStorage.removeItem(sessionRetryKey); } catch (_) {}
  }
  if (!response.ok) {
    throw new Error(result.error || 'Operazione non riuscita. Apri «Problemi» per i dettagli.');
  }
  return result;
}

// Disabilita il pulsante mentre l'azione è in corso e mostra eventuali errori.
async function perform(button, action) {
  const label = button.childElementCount === 0 ? button.textContent : null;
  button.disabled = true;
  if (label !== null) button.textContent = 'Attendi…';
  button.setAttribute('aria-busy', 'true');
  try {
    await action();
  } catch (error) {
    const modal = button.closest('dialog[open]');
    if (modal) {
      let notice = modal.querySelector('[role=alert]');
      if (!notice) { notice = element('p', undefined, 'card-error'); notice.setAttribute('role', 'alert'); modal.append(notice); }
      notice.textContent = error.message;
    } else if (!sessionLocked) showNotice(error.message, true);
  } finally {
    button.disabled = sessionLocked;
    button.removeAttribute('aria-busy');
    if (label !== null) button.textContent = label;
  }
}

function formatSize(bytes) {
  if (bytes == null) return 'Non disponibile';
  if (bytes < 1024) return bytes + ' B';
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
  return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
}

function formatDate(value) {
  if (!value) return 'Non ancora disponibile';
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return value;
  return date.toLocaleString('it-IT', { timeZone: 'UTC' }) + ' UTC';
}

function sourceError(source) {
  return source.error || (source.phase === 'error' ? source.message : null);
}

// Restituisce [testo, classe CSS] per il badge di una sorgente.
function sourceBadge(source) {
  if(source.detached)return ['Scollegata · storico disponibile','waiting'];

  if (!source.enabled) return [source.resume_at ? 'In pausa fino al ' + new Date(source.resume_at * 1000).toLocaleString('it-IT') : 'In pausa', 'waiting'];
  if (sourceError(source)) return ['Errore di lettura', 'error'];
  if (source.phase === 'collecting') return ['Lettura in corso', 'waiting'];
  if (!source.last_collection) return ['Prima lettura…', 'waiting'];
  return ['Collegata', ''];
}

// Mette in evidenza nel testo le occorrenze della parola cercata.
function highlight(parent, text, query) {
  const needle = query.trim();
  if (!needle) {
    parent.textContent = text;
    return;
  }
  const lowerText = text.toLowerCase();
  const lowerNeedle = needle.toLowerCase();
  let position = 0;
  while (position < text.length) {
    const found = lowerText.indexOf(lowerNeedle, position);
    if (found < 0) break;
    parent.append(text.slice(position, found));
    parent.append(element('mark', text.slice(found, found + needle.length)));
    position = found + needle.length;
  }
  parent.append(text.slice(position));
}

// ---------- elenco sorgenti ----------

// Le sonde del runtime possono essere lente: condividi la richiesta tra scheda e impostazioni.
const originalRetentionCache = new Map();

function originalRetentionNote(input, source) {
  const note = element('p', 'Verifica della durata degli originali…', 'muted');
  if (source && (input.kind === 'file' || input.kind === 'folder')) {
    // Per i file dice chi ripulisce gli originali e se due sistemi si sovrappongono.
    const key = JSON.stringify(['cleanup', source.source, input, source.remove_rotated, source.completed_dir, source.retention]);
    let cached = originalRetentionCache.get(key);
    if (!cached || Date.now() - cached.at > 60000) {
      cached = { at: Date.now(), result: api('cleanup', { name: source.source }).catch(() => null) };
      originalRetentionCache.set(key, cached);
    }
    cached.result.then((r) => {
      if (!r) { note.textContent = 'Pulizia degli originali: configurazione non disponibile.'; return; }
      const warns = r.notes.filter((n) => n.level === 'warn').map((n) => n.text);
      note.textContent = 'Originali · ' + r.headline + (warns.length ? ' ' + warns.join(' ') : '');
      note.className = r.level === 'warn' || warns.length ? 'card-warning' : 'muted';
    });
    return note;
  }
  const key = JSON.stringify([source?.source, input, source?.remove_rotated, source?.completed_dir]);
  let cached = originalRetentionCache.get(key);
  if (!cached || Date.now() - cached.at > 60000) {
    const request = source
      ? api('original-retention', { name: source.source })
      : api('preview-retention', input);
    cached = { at: Date.now(), result: request.catch(() => 'Configurazione non disponibile.') };
    originalRetentionCache.set(key, cached);
  }
  cached.result.then((policy) => {
    note.textContent = policy;
  });
  return note;
}

const LOG_KINDS = {
  access: 'Accessi web', error: 'Errori', application: 'Applicazione', database: 'Database',
  admin_access: 'Accessi amministrativi', system: 'Sistema', other: 'Altro',
};

function assignLogButton(log) {
  const button = element('button', log.site ? 'Sposta' : 'Assegna', 'admin-only');
  button.type = 'button';
  button.setAttribute('aria-label', button.textContent + ' ' + log.label);
  button.onclick = () => {
    $('move-name').textContent = 'Log selezionato · ' + log.label;
    $('move-log').dataset.source = log.source;
    $('move-site').replaceChildren(new Option('Scegli dove mostrarlo', ''), ...orderedSites()
      .filter(site => site.id !== log.site).map(site => new Option(site.name, site.id)));
    $('assign-dialog').showModal();
  };
  return button;
}
$('move-log').onsubmit = event => {
  event.preventDefault();
  perform(event.submitter, async () => {
    const site = $('move-site').value;
    await api('site-assign', {name: $('move-log').dataset.source, site});
    $('assign-dialog').close();
    await refresh(true);
    location.hash = '#site/' + site;
  });
};
for (const button of document.querySelectorAll('[data-close]')) button.onclick = () => button.closest('dialog').close();
for (const dialog of document.querySelectorAll('dialog')) dialog.addEventListener('close', () => {
  dialog.querySelector('p.card-error[role=alert]')?.remove();
});

function siteCard(site, logs) {
  const card = element('article', undefined, 'source-card');
  const header = element('div', undefined, 'card-heading');
  const title = element('h3');
  title.append(link(site.name, '#site/' + site.id, 'text-link'));
  const errors = logs.filter(s => sourceError(s)).length;
  const attention = logs.some(s => sourceBadge(s)[1] || s.warning || s.recovery ||
    Date.now() - Date.parse(s.last_collection) > 15 * 60000 ||
    s.gaps.some(gap => Date.now() - Date.parse(gap.to) < 86400000));
  let badge = ['Log aggiornati', ''];
  if (errors) badge = [errors + ' in errore', 'error'];
  else if (!logs.length) badge = ['Nessun log', 'waiting'];
  else if (attention) badge = ['Da controllare', 'attention'];
  header.append(title, element('span', badge[0], 'badge ' + badge[1]));
  card.append(header);
  if (site.id === '~') card.append(element('p', 'Scegli a quale sito o al server appartiene ciascun log.'));
  if (site.id === 'server') card.append(element('p', 'Accessi al sistema, servizi e attività pianificate.'));
  if (!logs.length) card.append(link('Aggiungi il primo log', '#add/' + site.id, 'button admin-only'));
  else {
    const list = element('ul', undefined, 'site-log-list');
    for (const log of logs) {
      const item = element('li');
      item.append(link(log.label, (document.body.classList.contains('reader') ? '#logs/' : '#settings/') + log.source, 'text-link'), document.createTextNode(' · ' + (LOG_KINDS[log.log_kind] || 'Altro')));
      if (site.id === '~') item.append(assignLogButton(log));
      if (log.gaps?.length && Date.now() - Date.parse(log.gaps[0].to) < 86400000) {
        item.append(element('small', 'Potrebbero mancare eventi. Apri il log per vedere il periodo.', 'card-warning'));
      }
      list.append(item);
    }
    card.append(list);
  }
  // Un errore senza spiegazione non aiuta: quale log, perché, e dove guardare.
  for (const log of logs.filter(s => sourceError(s))) {
    const problem = element('div', undefined, 'card-error');
    const raw = sourceError(log);
    const reason = raw.includes('os error 2') && ['file','folder'].includes(log.input.kind)
      ? 'Percorso non trovato. Controlla da dove leggere questo log.' : raw;
    problem.append(element('strong', (log.label || log.source) + ': '),
      document.createTextNode(reason.length > 160 ? reason.slice(0, 160) + '…' : reason + ' '));
    problem.append(link('Apri il log', '#settings/' + log.source, 'text-link'));
    card.append(problem);
  }
  if (logs.length) {
    const archive = logs.reduce((sum, s) => sum + s.archive_bytes, 0);
    const original = state.originals?.[site.id === '~' ? '' : site.id];
    if (original) card.append(element('p', 'Originali in raccolta · ' + formatSize(original.bytes) +
      (original.partial ? ' (misura parziale)' : '')));
    card.append(element('p', 'Archivio · ' + formatSize(archive)));
  }
  if (!document.body.classList.contains('reader')) {
    for (const log of logs.filter(log => log.entries)) card.append(adviceNote(log));
  }
  return card;
}

let detailSignature = '';
let previewSignature = '';
let previewRequest = 0;

function siteLogRow(source) {
  const title = element('div');
  title.append(link(sourceOptionLabel(source), '#logs/' + source.source, 'text-link'),
    element('small', LOG_KINDS[source.log_kind] || 'Altro'));
  title.append(element('small', source.origin, 'path'));
  const copies = copyLocations(source);
  // Il programma è solo un'etichetta: si riconosce dal percorso, dal servizio o dalla scelta fatta.
  const program = element('div', source.program || 'Non riconosciuto');
  program.append(element('small', source.format ? 'Formato · ' + source.format : 'Formato · in attesa delle prime righe'));
  const rotation = element('div', rotationText(source));
  const policy = element('div', periodText(source.retention));
  policy.append(element('small', source.class || 'Regola individuale'));
  const [label, style] = sourceBadge(source);
  const status = element('div');
  status.append(element('span', label, 'badge ' + style));
  if (source.last_collection) status.append(element('small', formatDate(source.last_collection)));
  for (const message of [sourceError(source), source.warning, source.recovery].filter(Boolean)) {
    status.append(element('small', message));
  }
  for (const gap of source.gaps.slice(0, 3)) {
    status.append(element('small', 'Potrebbero mancare eventi · ' + formatDate(gap.from) + ' – ' + formatDate(gap.to) + ' · ' + gap.reason, 'card-warning'));
  }
  const history = element('details');
  history.dataset.key = 'history-' + source.source;
  history.append(element('summary', 'Storico'), element('small', source.entries + ' righe'),
    element('small', 'Prima riga · ' + formatDate(source.earliest)), element('small', source.message));
  status.append(history);
  const actions = element('div');
  actions.append(assignLogButton(source));
  if (source.log_kind === 'access') actions.append(link('Statistiche', '#stats/' + source.source, 'text-link'));
  actions.append(link('Impostazioni', '#settings/' + source.source, 'text-link admin-only'));
  if (source.entries && !document.body.classList.contains('reader')) actions.append(adviceNote(source));
  return [title, program, rotation, copies, policy, status, actions];
}

function renderSite(id) {
  const site = id === '~' ? { name: 'Log da sistemare' } : Object.hasOwn(state.sites, id) && state.sites[id];
  $('page-title').textContent = site ? site.name : 'Gruppo non trovato';
  document.title = 'Loglem · ' + $('page-title').textContent;
  const logs = state.sources.filter(s => s.site === (id === '~' ? '' : id));
  const signature = JSON.stringify([id, site, logs]);
  updateCopyScope(id, !!site && !!logs.length, logs);
  if (signature === detailSignature) return;
  detailSignature = signature;
  const actions = $('site-actions');
  actions.replaceChildren();
  const opened = new Set([...$('site-logs').querySelectorAll('details[open]')].map(d => d.dataset.key));
  $('site-logs').replaceChildren();
  if (!site) {
    $('site-preview').hidden = true;
    return;
  }
  actions.append(link('Aggiungi un log', id === '~' ? '#add' : '#add/' + id, 'button primary admin-only'));
  if (logs.length) {
    actions.append(link('Cerca in questi log', '#logs/site/' + id));
    tabulate('site-logs', ['Log e origine', 'Programma', 'Gestione dei vecchi file', 'Copie', 'Durata in Loglem', 'Stato', 'Azioni'], logs.map(siteLogRow));
    for (const detail of $('site-logs').querySelectorAll('details')) detail.open = opened.has(detail.dataset.key);
  } else {
    $('site-logs').append(element('p', 'Nessun log collegato.'));
  }
  refreshSitePreview(id, logs);
}

async function refreshSitePreview(id, logs) {
  const signature = JSON.stringify([id, logs.map(s => [s.source, s.entries, s.earliest])]);
  if (signature === previewSignature) return;
  previewSignature = signature;
  const request = ++previewRequest;
  const panel = $('site-preview');
  panel.hidden = !logs.some(s => s.entries > 0);
  if (panel.hidden) return;
  const list = $('site-recent');
  list.replaceChildren();
  try {
    const { rows } = await api('search', { site: id, limit: 3 });
    if (request === previewRequest && location.hash === '#site/' + id) {
      list.replaceChildren(...rows.map(row => renderLogRow(row, '')));
    }
  } catch (error) {
    if (request === previewRequest) {
      list.textContent = error.message;
      previewSignature = '';
    }
  }
}

function collectionStatusText(progress) {
  if (progress.running) return 'Lettura dei log in corso…';
  if (progress.requested) return 'Lettura richiesta, sta per partire…';
  if (progress.error) {
    return 'Alcune sorgenti non sono state lette. Il motivo è scritto nella loro scheda.';
  }
  return 'Lettura automatica attiva · ultima lettura: ' + formatDate(progress.last_finished);
}

function renderSources() {
  const sources = state.sources;
  $('source-count').textContent = sources.length;
  $('entry-count').textContent = sources
    .reduce((total, s) => total + s.entries, 0)
    .toLocaleString('it-IT');
  $('archive-size').textContent = formatSize(sources.reduce((total, s) => total + s.archive_bytes, 0));
  $('collection-status').textContent = collectionStatusText(state.collection);

  // Ridisegna le schede solo se i dati sono cambiati, per non perdere i «details» aperti.
  const signature = JSON.stringify([state.sites, state.originals, sources]);
  if (signature === lastSignature) return;
  lastSignature = signature;

  const list = $('source-list');
  list.replaceChildren();
  const groups = new Map();
  for (const source of sources) {
    if (!groups.has(source.site)) groups.set(source.site, []);
    groups.get(source.site).push(source);
  }
  for (const site of orderedSites()) list.append(siteCard(site, groups.get(site.id) || []));
  if (groups.has('')) list.append(siteCard({id: '~', name: 'Log da sistemare'}, groups.get('')));
}

function sourceOptionLabel(source) {
  return (source.label || source.source) + (source.detached ? ' (storico)' : '');
}

// Riquadro in cima alla pagina: sorgenti ferme, in errore o con lacune recenti.
function renderProblems(problems) {
  const box = $('problems');
  if (problems.length === 0) {
    // Nessun problema: la fascia dice comunque che tutto va bene (o perché non fidarsi).
    calmStatus(box);
    return;
  }
  box.className = 'problems bad';
  box.hidden = false;
  box.replaceChildren();
  box.append(element('strong', problems.length === 1 ? 'Attenzione' : problems.length + ' cose da controllare'));
  const list = element('ul');
  for (const problem of problems.slice(0, 5)) {
    const item = element('li', problem.text + ' ');
    const seen = element('button', 'Segna come visto', 'link-button');
    seen.type = 'button';
    seen.addEventListener('click', () =>
      perform(seen, async () => {
        await api('alert-seen', { id: problem.id });
        await refresh();
      }),
    );
    item.append(seen);
    list.append(item);
  }
  box.append(list);
  const mail = emailWarning();
  if (mail) box.append(element('p', mail));
}

function orderedSites() {
  return Object.values(state.sites).sort((a, b) => {
    if ((a.id === 'server') !== (b.id === 'server')) return a.id === 'server' ? 1 : -1;
    return a.name.localeCompare(b.name);
  });
}

function siteInputValue(site) {
  const name = site.name;
  const conflicts = Object.hasOwn(state.sites, name) && name !== site.id;
  const repeated = Object.values(state.sites).some(other => other.id !== site.id && other.name === name);
  return conflicts || repeated || name.trim() !== name ? site.id : name;
}

function syncSourceSelect() {
  const select = $('search-source');
  const previous = select.value;
  // Ricostruisce l'elenco solo se nomi o etichette sono cambiati.
  const sites = orderedSites();
  if (state.sources.some(s => !s.site)) sites.push({ id: '~', name: 'Log da sistemare' });
  const site = $('search-site');
  const options = [...site.options].slice(1);
  if (sites.length !== options.length || sites.some((s, i) => s.id !== options[i].value || s.name !== options[i].text)) {
    const chosen = site.value;
    site.replaceChildren(new Option('Tutti i gruppi', ''), ...sites.map(s => new Option(s.name, s.id)));
    if (sites.some(s => s.id === chosen)) site.value = chosen;
  }
  const sources = state.sources.filter(s => !site.value || s.site === (site.value === '~' ? '' : site.value));
  const wanted = JSON.stringify(sources.map(s => [s.source, sourceOptionLabel(s)]));
  const current = JSON.stringify([...select.options].slice(1).map((o) => [o.value, o.textContent]));
  if (wanted === current) return;

  select.replaceChildren(new Option('Tutte le sorgenti', ''));
  for (const source of sources) {
    select.add(new Option(sourceOptionLabel(source), source.source));
  }

  let chosen = previous;
  if (!chosen && location.hash.startsWith('#logs/')) {
    try {
      chosen = decodeURIComponent(location.hash.slice(6));
    } catch (_) {
      /* indirizzo non valido: si lascia «Tutte le sorgenti» */
    }
  }
  if (sources.some((s) => s.source === chosen)) select.value = chosen;
}

function refresh(afterMutation = false) {
  if (statusRequest) return afterMutation === true ? statusRequest.then(() => refresh()) : statusRequest;
  statusRequest = readStatus(afterMutation === 'poll').finally(() => { statusRequest = null; });
  return statusRequest;
}

async function readStatus(poll = false) {
  try {
    const generation = state.collection.generation;
    const next = await api('status', poll && generation !== undefined ? { generation } : {});
    if (next.unchanged) {
      state.collection = next.collection;
      if (next.live) state.live = next.live;
      renderLive();
      $('connection-status').textContent = 'Collegato';
      $('footer-status').textContent = 'in funzione';
      if (view === 'sources') $('collection-status').textContent = collectionStatusText(next.collection);
      return;
    }
    state = next;
    $('version').textContent = 'Versione ' + state.version;
    $('data-dir').textContent = state.archive_directory;
    $('connection-status').textContent = 'Collegato';
    $('footer-status').textContent = 'in funzione';
    syncSourceSelect();
    renderProblems(state.problems || []);
    renderLive();
    if (view === 'settings' && settingsSource) {
      const current = state.sources.find(s => s.source === settingsSource.source);
      if (current) {
        // Refresh lifecycle actions without replacing the user's draft form values.
        for (const key of ['enabled','detached','resume_at','deleted_originals','gaps']) settingsSource[key] = current[key];
        updateLifecycle(settingsSource);
      } else {
        route();
      }
    }
    if (view === 'sources') renderSources();
    if (view === 'site') renderSite(location.hash.split('/')[1]);
    if (view === 'logs') {
      renderLogStatus();
    }
  } catch (error) {
    $('connection-status').textContent = 'Non collegato';
    $('footer-status').textContent = error.message;
    if (view === 'sources' || view === 'logs') showNotice(error.message, true);
  }
}

// ---------- ricerca nei log ----------

function renderLogStatus() {
  const name = $('search-source').value;
  const source = state.sources.find((s) => s.source === name);
  const error = source ? sourceError(source) : state.collection.error;
  const status = $('log-source-status');
  status.className = 'inline-status' + (error ? ' error' : '');

  if (error) {
    status.textContent = 'Lettura non riuscita: ' + error + ' I log già salvati restano consultabili.';
  } else if (state.collection.running) {
    status.textContent = 'Lettura in corso: le nuove righe compariranno da sole.';
  } else if (source && !source.last_collection) {
    status.textContent = 'In attesa della prima lettura…';
  } else if (source && source.entries === 0) {
    status.textContent = 'Questa sorgente non ha ancora prodotto righe nel periodo conservato.';
  } else {
    status.textContent = 'Le righe più recenti sono in alto. Gli orari sono in UTC.';
  }
}

// Il colore da solo non basta a chi non lo distingue: ogni riga porta anche la parola.
const LEVEL_LABELS = { info: 'INFO', warning: 'AVVISO', error: 'ERRORE' };
const LEVEL_CHECKS = ['info', 'warning', 'error'];

// Le gravità da mostrare: vuoto = tutte (nessun filtro).
function chosenLevels() {
  const chosen = LEVEL_CHECKS.filter((name) => $('show-' + name).checked);
  return chosen.length === LEVEL_CHECKS.length ? [] : chosen;
}

for (const name of LEVEL_CHECKS) {
  $('show-' + name).addEventListener('change', () => {
    // Almeno una deve restare scelta: senza nessuna non ci sarebbe niente da vedere.
    if (LEVEL_CHECKS.every((n) => !$('show-' + n).checked)) $('show-' + name).checked = true;
    search();
  });
}

function renderLogRow(row, query) {
  const line = element('div', undefined, 'log-row');
  const stamp = element('div', formatDate(row.timestamp), 'log-date');
  if (row.timestamp_origin !== 'event') {
    stamp.append(element('small', 'Data di lettura (il log non indica la sua data)'));
  }
  const message = element('pre', undefined, 'log-text');
  highlight(message, row.message, query);
  const level = LEVEL_LABELS[row.level] ? row.level : 'info';
  line.classList.add('level-' + level);
  stamp.append(element('span', LEVEL_LABELS[level], 'log-level'));
  line.append(stamp, element('div', row.source, 'log-source'), message);
  return line;
}

function renderNoResults(list) {
  const hasSources = state.sources.length > 0;
  const box = element('div', undefined, 'logs-empty');
  box.append(
    element('h3', hasSources ? 'Nessuna riga trovata' : 'Prima devi collegare una sorgente'),
    element(
      'p',
      hasSources
        ? 'Prova con un’altra parola, togli le date oppure scegli «Tutte le sorgenti». ' +
            'Se la lettura è appena partita, attendi qualche secondo.'
        : 'Aggiungi una cartella di file .log o un container: poi i suoi log compariranno qui.',
    ),
  );
  if (!hasSources) box.append(link('Aggiungi una sorgente', '#add', 'button primary admin-only'));
  list.append(box);
}

async function search(showErrors = true) {
  if (searching && !showErrors) return;
  searching = true;
  const sequence = ++searchSequence;
  const query = $('search-text').value;
  try {
    const { rows, ranking } = await api('search', {
      site:$('search-site').value, question:$('search-question').value,
      ...currentQuery(),
      from: $('search-from').value,
      to: $('search-to').value,
    });
    if (sequence !== searchSequence) return; // è partita una ricerca più recente

    $('result-count').textContent =
      rows.length === 0
        ? 'Nessun risultato'
        : rows.length + (rows.length === 1 ? ' riga' : ' righe') + ' (le più recenti, fino a 200)';
    const list = $('search-results');
    list.replaceChildren();
    if ($('search-question').value === '404') {
      $('result-count').textContent = (ranking || []).length + ' indirizzi IP';
      for (const [ip,count] of ranking || []) list.append(element('p',ip+' · '+count+' risposte 404'));
    }
    if (rows.length === 0 && !ranking?.length) renderNoResults(list);
    for (const row of rows) list.append(renderLogRow(row, query));
    startLive();
  } catch (error) {
    if (sequence !== searchSequence) return;
    $('result-count').textContent = 'Ricerca non riuscita';
    if (showErrors) showNotice(error.message, true);
    $('log-source-status').textContent = error.message;
    $('log-source-status').className = 'inline-status error';
  } finally {
    if (sequence === searchSequence) searching = false;
  }
}

// ---------- diretta ----------

const LIVE_BUFFER = 500; // righe tenute nella pagina in diretta
let liveRun = 0; // cresce a ogni ripartenza: il ciclo precedente si ferma da solo

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// La diretta ha senso solo guardando i log senza una data di fine.
function liveWanted() {
  return !$('search-site').value && !$('search-question').value && view === 'logs' && $('live').checked && !$('search-to').value;
}

function stopLive() {
  liveRun++;
}

// Chiede al server le righe nuove (ogni richiesta aspetta fino a 15 secondi)
// e le aggiunge in cima, con gli stessi filtri della ricerca.
async function startLive() {
  const run = ++liveRun;
  if (!liveWanted()) return;
  const filters = currentQuery();
  let after = null;
  while (run === liveRun && liveWanted()) {
    try {
      const reply = await api('tail', { ...filters, after });
      if (run !== liveRun) return;
      if (after !== null) addLiveRows(reply.rows, filters.text);
      after = reply.after;
    } catch (_) {
      await sleep(3000); // server riavviato o rete assente: riprova
    }
  }
}

function addLiveRows(rows, query) {
  if (rows.length === 0) return;
  const list = $('search-results');
  list.querySelector('.logs-empty')?.remove();
  for (const row of rows) list.prepend(renderLogRow(row, query)); // la più nuova in cima
  while (list.children.length > LIVE_BUFFER) list.lastChild.remove();
  $('result-count').textContent = list.children.length + ' righe · in diretta';
}

// Testo e filtri della ricerca, come li vuole il server (vale anche per la diretta).
function currentQuery() {
  const status = $('search-status').value;
  return {
    source: $('search-source').value,
    text: $('search-text').value,
    ignore_case: $('search-ignore-case').checked,
    only: chosenLevels(),
    ip: $('search-ip').value.trim() || null,
    path: $('search-path').value.trim() || null,
    status: status ? Number(status) : null,
  };
}

function clearSearch() {
  if (location.hash.startsWith('#logs/')) history.replaceState(null, '', '#logs');
  $('search-site').value=''; $('search-question').value=''; $('live').disabled=false;
  syncSourceSelect();
  $('search-source').value = '';
  for (const id of ['search-ip', 'search-path', 'search-status']) $(id).value = '';
  for (const name of LEVEL_CHECKS) $('show-' + name).checked = true;
  $('search-text').value = '';
  $('search-from').value = '';
  $('search-to').value = '';
  search();
}

// ---------- suggerimenti sui dati personali ----------

// Loglem guarda le righe recenti di una sorgente e propone la scelta prudente, con il
// riferimento di legge. Le scelte restano dell'utente: applica, fatto, più tardi, non mi riguarda.

const ADVICE_STATE = {
  done: ['Fatto', ''],
  declined: ['Non ti riguarda', 'waiting'],
  later: ['Rimandato', 'waiting'],
};
const adviceCache = new Map();
let adviceWatch = 0;
let adviceSite = null;

function adviceItem(item, name, compact, reload) {
  const box = element('article', undefined, 'advice-item' + (item.important ? ' important' : ''));
  box.dataset.state = item.state;
  const head = element('div', undefined, 'advice-head');
  head.append(element('h4', item.title));
  const label = ADVICE_STATE[item.state];
  if (label) head.append(element('span', label[0], 'badge ' + label[1]));
  else if (item.important) head.append(element('span', 'Importante', 'badge error'));
  box.append(head);

  const closed = item.state === 'done' || item.state === 'declined';
  if (closed && item.state === 'done' && item.done_text) {
    box.append(element('p', item.done_text, 'muted'));
  }
  const body = closed ? element('details') : box;
  if (closed) {
    body.append(element('summary', 'Perché e cosa proponevo'));
    box.append(body);
  }
  body.append(
    element('p', item.why),
    element('p', item.proposal),
    element('p', item.verify, 'advice-verify'),
  );
  if (item.refs.length) {
    const refs = element('p', 'Riferimenti: ', 'advice-refs');
    item.refs.forEach((r, i) => {
      const a = element('a', r.law + ', ' + r.text);
      a.href = r.url;
      a.target = '_blank';
      a.rel = 'noopener noreferrer';
      if (i) refs.append(' · ');
      refs.append(a);
    });
    body.append(refs);
  }

  const actions = element('div', undefined, 'row advice-actions');
  const choose = (labelText, choice, primary = false) => {
    const b = element('button', labelText, primary ? 'primary' : '');
    b.type = 'button';
    b.addEventListener('click', () =>
      perform(b, async () => {
        await api('advice-choice', { name, id: item.id, choice });
        await reload();
      }),
    );
    return b;
  };
  if (closed || item.state === 'later') {
    if (!compact) actions.append(choose('Rivedi', 'reopen'));
  } else {
    if (item.action === 'mask_ips' || item.action === 'retention') {
      const b = element('button', item.action_label, 'primary');
      b.type = 'button';
      b.addEventListener('click', () =>
        perform(b, async () => {
          if (
            item.action === 'mask_ips' &&
            !confirm('Anche le righe già archiviate verranno mascherate, e non si potrà tornare indietro. Continuare?')
          ) {
            return;
          }
          const result = await api('advice-apply', { name, id: item.id });
          showNotice(result.message);
          await refresh();
          await reload();
        }),
      );
      actions.append(b);
    } else if (item.action === 'open_privacy') {
      actions.append(link(item.action_label, '#privacy', 'button primary'));
    }
    actions.append(
      choose('Ho già provveduto', 'done'),
      choose('Ricordamelo più tardi', 'later'),
      choose('Non mi riguarda', 'declined'),
    );
  }
  if (actions.children.length) box.append(actions);
  return box;
}

// `compact`: solo i suggerimenti aperti (pannello dopo l'aggiunta di una sorgente).
function renderAdvice(box, report, name, compact, reload) {
  box.replaceChildren();
  const items = compact ? report.items.filter((i) => i.state === 'open') : report.items;
  if (compact && items.length === 0) {
    box.hidden = true;
    return;
  }
  box.hidden = false;
  const head = element('div', undefined, 'section-heading');
  head.append(element('h3', compact ? 'Cosa ho trovato nel log «' + name + '»' : 'Dati personali in questo log'));
  if (compact) {
    const close = element('button', 'Chiudi');
    close.type = 'button';
    close.addEventListener('click', () => (box.hidden = true));
    head.append(close);
  }
  box.append(head);
  if (!report.findings) {
    box.append(element('p', 'Sto aspettando le prime righe per capire cosa contiene questo log.', 'muted'));
    return;
  }
  if (report.items.length === 0) {
    const n = report.findings.lines_read;
    box.append(element('p', (n === 1 ? 'Nell’ultima riga' : 'Nelle ultime ' + n + ' righe') + ' non ho trovato dati personali.', 'muted'));
    return;
  }
  box.append(
    element(
      'p',
      (report.findings.lines_read === 1 ? 'Ho guardato l’ultima riga.' : 'Ho guardato le ultime ' + report.findings.lines_read + ' righe.') +
        ' Ecco cosa ti suggerisco: ' +
        'ogni proposta si applica con un tasto, oppure scegli tu.',
      'muted',
    ),
  );
  const list = element('div', undefined, 'advice-list');
  for (const item of items) list.append(adviceItem(item, name, compact, reload));
  box.append(list);
  const foot = element('div', undefined, 'advice-foot');
  foot.append(element('p', report.disclaimer));
  if (report.unverified) foot.append(element('p', report.unverified, 'advice-unverified'));
  foot.append(element('p', 'Testi rivisti il ' + report.revised + '.'));
  box.append(foot);
}

async function loadAdvice(name, box, compact) {
  const reload = () => loadAdvice(name, box, compact);
  try {
    const report = await api('advice', { name });
    rememberAdvice(name, report);
    renderAdvice(box, report, name, compact, reload);
    return report;
  } catch (error) {
    box.hidden = true;
    return null;
  }
}

// Dopo aver aggiunto una sorgente: aspetta le prime righe, poi mostra i suggerimenti aperti.
async function watchNewSource(name, site) {
  const run = ++adviceWatch;
  adviceSite = site;
  const panel = $('advice-panel');
  panel.hidden = true;
  for (let i = 0; i < 40 && run === adviceWatch; i++) {
    await sleep(3000);
    if (run !== adviceWatch || location.hash !== '#site/' + site) return;
    const report = await loadAdvice(name, panel, true);
    if (location.hash !== '#site/' + site) panel.hidden = true;
    if (!report || report.findings) return;
    panel.hidden = true; // ancora nessuna riga: niente pannello vuoto
  }
}

function rememberAdvice(name, report) {
  const items = report.items.filter(item => item.state === 'open');
  const titles = items.map(item => item.title);
  adviceCache.set(name, { at: Date.now(), titles });
  return titles;
}

// Share the sample between the site's card and its detail page.
const pendingAdvice = new Map();
function adviceNote(source) {
  const note = element('p', undefined, 'advice-note admin-only');
  const show = titles => {
    note.hidden = !titles.length;
    if (!titles.length) return;
    const text = titles[0] + (titles.length > 1 ? ' · +' + (titles.length - 1) : '');
    const action = link(text, '#settings/' + source.source, 'text-link');
    action.title = sourceOptionLabel(source) + ' · ' + titles.join(' · ');
    note.replaceChildren(action);
  };
  const name = source.source;
  const cached = adviceCache.get(name);
  show(cached?.titles || []);
  if (cached && Date.now() - cached.at <= 60000) return note;
  if (!pendingAdvice.has(name)) {
    const request = api('advice', {name})
      .then(report => rememberAdvice(name, report))
      .catch(() => {
        const titles = cached?.titles || [];
        adviceCache.set(name, {at: Date.now(), titles});
        return titles;
      })
      .finally(() => pendingAdvice.delete(name));
    pendingAdvice.set(name, request);
  }
  pendingAdvice.get(name).then(show);
  return note;
}

// ---------- statistiche ----------

let statsSource = '';

// Una tabella semplice; `bar` aggiunge una barra proporzionale al numero.
function statsTable(title, rows, bar = false) {
  const box = element('div', undefined, 'stats-box');
  box.append(element('h3', title));
  if (rows.length === 0) {
    box.append(element('p', 'Nessun dato.', 'muted'));
    return box;
  }
  const max = Math.max(...rows.map((r) => r[1]));
  const table = element('table');
  for (const [label, value, extra] of rows) {
    const tr = element('tr');
    const name = element('td', String(label));
    const cell = element('td', extra === undefined ? value.toLocaleString('it-IT') : extra);
    if (bar) {
      const fill = element('span', undefined, 'stats-bar');
      fill.style.width = Math.max(2, (value / max) * 100) + '%';
      name.append(fill);
    }
    tr.append(name, cell);
    table.append(tr);
  }
  box.append(table);
  return box;
}

async function loadStats() {
  const source = state.sources.find((s) => s.source === statsSource);
  $('stats-title').textContent = 'Statistiche · ' + (source ? source.label || source.source : statsSource);
  $('stats-body').replaceChildren(element('p', 'Calcolo…', 'muted'));
  try {
    const stats = await api('stats', { source: statsSource, days: Number($('stats-days').value) });
    $('stats-summary').textContent =
      stats.requests.toLocaleString('it-IT') + ' richieste riconosciute' +
      (stats.approximate ? ' · Classifica approssimata' : '') +
      (stats.other_lines ? ' · ' + stats.other_lines.toLocaleString('it-IT') + ' righe di altro formato' : '');
    const body = $('stats-body');
    body.replaceChildren(
      statsTable('Richieste per giorno', stats.per_day, true),
      statsTable('Pagine più richieste', stats.top_paths, true),
      statsTable('Codici di risposta', stats.statuses, true),
      statsTable('Indirizzi più frequenti', stats.top_ips, true),
    );
    if (stats.slowest.length) {
      const rows = stats.slowest.map(([path, ms, count]) => [path, count, ms.toFixed(2) + ' ms']);
      body.append(statsTable('Pagine più lente (media)', rows));
    }
    if (stats.requests === 0) {
      body.replaceChildren(
        element('p', 'Nessuna richiesta HTTP riconosciuta: le statistiche valgono per i log di ' +
          'accesso con codice e percorso, in JSON, common/combined o logfmt, di qualsiasi programma.', 'muted'),
      );
    }
  } catch (error) {
    $('stats-body').replaceChildren(element('div', error.message, 'card-error'));
  }
}

$('stats-days').addEventListener('change', loadStats);

// ---------- dati personali: cancellare, esportare, registro ----------

let lastFound = null; // cosa ha trovato l'ultima anteprima

function privacyRequest() {
  return {
    source: $('privacy-source').value,
    ip: $('privacy-ip').value.trim() || null,
    text: $('privacy-text').value.trim(),
    originals: $('privacy-originals').checked,
  };
}

function prepareProtection() {
  const select = $('privacy-source');
  const wanted = select.value;
  select.replaceChildren(new Option('Tutte', ''));
  for (const s of state.sources) select.add(new Option(s.label || s.source, s.source));
  select.value = wanted;
  loadAudit();
}

const FILE_STATUS = {
  would_rewrite: 'verrebbe riscritto',
  rewritten: 'riscritto',
  skipped: 'saltato',
};

function renderFound(found, done) {
  const box = $('privacy-result');
  box.replaceChildren();
  const total = (found.counts || []).reduce((n, c) => n + c.rows, 0);
  box.append(element('h3', done ? 'Cancellate ' + found.rows + ' righe' : total + ' righe nell’archivio di Loglem'));
  if (!done) {
    const list = element('ul');
    for (const c of found.counts) list.append(element('li', c.source + ': ' + c.rows + ' righe'));
    box.append(list);
  }
  for (const group of found.originals || []) {
    for (const file of group.files.filter((f) => f.status !== 'nothing')) {
      let text = group.source + ' · ' + file.file.split('/').pop() + ': ' + (FILE_STATUS[file.status] || file.status);
      if (file.matched) text += ' (' + file.matched + ' righe)';
      if (file.note) text += ' — ' + file.note;
      box.append(element('p', text, file.status === 'skipped' ? 'muted' : ''));
    }
  }
  if (done && found.resealed_days) {
    box.append(element('p', 'Modalità prova: ricalcolate le impronte di ' + found.resealed_days + ' giorni.', 'card-info'));
  }
  box.hidden = false;
  $('privacy-erase').hidden = done || total === 0 && !(found.originals || []).some((g) => g.files.some((f) => f.status === 'would_rewrite'));
}

$('privacy-form').addEventListener('submit', (event) => {
  event.preventDefault();
  perform(event.submitter, async () => {
    const request = privacyRequest();
    if (!request.ip && !request.text) throw new Error('Scrivi un indirizzo IP o un testo da cercare.');
    lastFound = await api('erase-preview', request);
    renderFound(lastFound, false);
  });
});

$('privacy-export').addEventListener('click', () =>
  perform($('privacy-export'), async () => {
    const request = privacyRequest();
    if (!request.ip && !request.text) throw new Error('Scrivi un indirizzo IP o un testo da cercare.');
    const { rows, text } = await api('export', request);
    if (rows === 0) {
      showNotice('Nessuna riga trovata: non c’è nulla da scaricare.');
      return;
    }
    const link = element('a');
    link.href = URL.createObjectURL(new Blob([text], { type: 'text/plain' }));
    link.download = 'dati-persona.txt';
    link.click();
    URL.revokeObjectURL(link.href);
    showNotice('Scaricate ' + rows + ' righe. Cancella il file dopo averlo inviato.');
  }),
);

$('privacy-erase-button').addEventListener('click', () =>
  perform($('privacy-erase-button'), async () => {
    const request = { ...privacyRequest(), reason: $('privacy-reason').value.trim() };
    if (!request.reason) throw new Error('Scrivi il motivo (per esempio il numero della pratica).');
    const question =
      'Cancellare queste righe da Loglem' +
      (request.originals ? ' e dai file originali' : '') +
      '? Non si può annullare.';
    if (!confirm(question)) return;
    const done = await api('erase', request);
    renderFound(done, true);
    $('privacy-reason').value = '';
    await refresh();
    loadAudit();
  }),
);

async function loadAudit() {
  const list = $('audit-list');
  try {
    const { entries } = await api('audit');
    list.replaceChildren();
    if (entries.length === 0) {
      list.append(element('p', 'Ancora nessuna voce.', 'muted'));
      return;
    }
    const table = element('table', undefined, 'audit-table');
    for (const e of entries.slice(0, 100)) {
      const row = element('tr');
      row.append(
        element('td', formatDate(e.at)),
        element('td', e.user),
        element('td', e.action),
        element('td', e.detail),
      );
      table.append(row);
    }
    list.append(table);
  } catch (error) {
    list.replaceChildren(element('div', error.message, 'card-error'));
  }
}

$('audit-reload').addEventListener('click', () => perform($('audit-reload'), loadAudit));

// ---------- sistema: aggiornamenti e impostazioni ----------

async function showUpdateStatus() {
  try {
    const { status, version } = await api('update-status');
    $('update-status').textContent = 'Versione installata: ' + version + ' · ' + status;
  } catch (error) {
    $('update-status').textContent = error.message;
  }
}

$('update-check').addEventListener('click', () =>
  perform($('update-check'), async () => {
    const { installed, latest } = await api('update-check');
    const newer = latest !== installed;
    $('update-status').textContent = newer
      ? 'Disponibile la versione ' + latest + ' (installata: ' + installed + ').'
      : 'Hai già l’ultima versione (' + installed + ').';
    $('update-install').hidden = !newer;
  }),
);

// Durante l'aggiornamento il servizio si riavvia: si aspetta che torni e si ricarica la pagina.
$('update-install').addEventListener('click', () =>
  perform($('update-install'), async () => {
    if (!confirm('Installare la nuova versione? Loglem si riavvierà.')) return;
    await api('update-install');
    for (let i = 0; i < 120; i++) {
      await sleep(2000);
      try {
        const { status, version } = await api('update-status');
        $('update-status').textContent = status;
        if (/non riuscito|failed/i.test(status)) return;
        if (version !== state.version) {
          location.reload();
          return;
        }
      } catch (_) {
        $('update-status').textContent = 'Loglem si sta riavviando…';
      }
    }
  }),
);

$('config-export').addEventListener('click', () =>
  perform($('config-export'), async () => {
    const config = await api('config-export');
    const file = new Blob([JSON.stringify(config, null, 2)], { type: 'application/json' });
    const link = element('a');
    link.href = URL.createObjectURL(file);
    link.download = 'loglem-impostazioni.json';
    link.click();
    URL.revokeObjectURL(link.href);
  }),
);

$('config-file').addEventListener('change', async () => {
  const file = $('config-file').files[0];
  $('config-file').value = '';
  if (!file) return;
  try {
    const config = JSON.parse(await file.text());
    const count = (config.sources || []).length;
    if (!confirm('Sostituire le impostazioni attuali con quelle del file (' + count + ' sorgenti)?')) return;
    const { message } = await api('config-import', { config });
    await refresh();
    showNotice(message);
  } catch (error) {
    showNotice(error instanceof SyntaxError ? 'Il file non è un JSON valido.' : error.message, true);
  }
});

// ---------- problemi (diagnostica) ----------

async function loadDiagnostics() {
  try {
    const data = await api('diagnostics');
    $('diagnostic-path').textContent = 'File del registro: ' + data.path;
    const list = $('diagnostic-list');
    list.replaceChildren();
    if (data.events.length === 0) {
      list.append(element('div', 'Nessun problema registrato. Tutto bene!', 'empty-state'));
      return;
    }
    for (const event of data.events) {
      const details = element('details', undefined, 'diagnostic');
      const summary = element('summary');
      const where =
        event.component + ' / ' + event.operation + (event.source ? ' · sorgente ' + event.source : '');
      summary.append(
        element('strong', event.message),
        element('small', formatDate(event.timestamp) + ' · ' + where),
      );
      details.append(summary, element('pre', JSON.stringify(event, null, 2)));
      list.append(details);
    }
  } catch (error) {
    showNotice(error.message, true);
  }
}

// ---------- navigazione ----------

function route() {
  const [requested, id = '', scope] = location.hash.slice(1).split('/');
  view = Object.hasOwn(PAGES, requested) ? requested : 'sources';

  for (const section of document.querySelectorAll('[data-view]')) {
    section.hidden = section.dataset.view !== view;
  }
  const activeNav = ['settings', 'site', 'new-site'].includes(view) ? 'sources' : view;
  for (const item of document.querySelectorAll('[data-nav]')) {
    if (item.dataset.nav === activeNav) item.setAttribute('aria-current', 'page');
    else item.removeAttribute('aria-current');
  }

  const [title, subtitle] = PAGES[view];
  $('page-title').textContent = title;
  $('page-subtitle').textContent = subtitle;
  $('breadcrumb').textContent = title.toUpperCase();
  $('header-add').hidden = view !== 'sources';
  document.title = 'Loglem · ' + title;
  hideNotice();
  stopLive();

  if (view === 'logs') {
    const siteSearch = id === 'site' && scope;
    if (siteSearch && ![...$('search-site').options].some(o => o.value === scope)) {
      $('search-results').replaceChildren();
      $('result-count').textContent = 'Gruppo non trovato';
      return;
    }
    if (id) {
      $('search-site').value = siteSearch ? scope : '';
      $('search-question').value = '';
      $('live').disabled = !!siteSearch;
      if (siteSearch) $('live').checked = false;
      syncSourceSelect();
    }
    try {
      $('search-source').value = id && !siteSearch ? decodeURIComponent(id) : '';
    } catch (_) {
      $('search-source').value = '';
    }
    renderLogStatus();
    search();
  } else if (view === 'sources') {
    lastSignature = '';
    renderSources();
  } else if (view === 'site') {
    copyScope = '';
    if (adviceSite !== id) $('advice-panel').hidden = true;
    detailSignature = '';
    previewSignature = '';
    renderSite(id);
  } else if (view === 'new-site') {
    $('new-site-form').reset();
    $('new-site-name').focus();
  } else if (view === 'add') {
    if (id && !Object.hasOwn(state.sites, id)) {
      location.hash = '#sources';
      return;
    }
    if (id) $('page-title').textContent = 'Aggiungi un log · ' + state.sites[id].name;
    openAdd(id);
  } else if (view === 'settings') {
    try {
      settingsSource = state.sources.find((s) => s.source === decodeURIComponent(id));
    } catch (_) {
      settingsSource = null;
    }
    $('retention-form').hidden = !settingsSource;
    $('settings-title').textContent = settingsSource
      ? settingsSource.label || settingsSource.source
      : 'Sorgente non trovata';
    $('settings-back').href = settingsSource ? '#site/' + (settingsSource.site || '~') : '#sources';
    $('settings-back').textContent = settingsSource ? '← ' + (state.sites[settingsSource.site]?.name || 'Log da sistemare') : '← Siti e server';
    if (settingsSource) populateSettings();
  } else if (view === 'rules' || view === 'system') {
    loadRulesPage().catch(e=>showNotice(e.message,true));
    if (view === 'system') {
      showUpdateStatus();
      loadSystemPanels();
    }
  } else if (view === 'diagnostics') {
    loadDiagnostics();
  } else if (view === 'privacy') {
    prepareProtection();

  } else if (view === 'stats') {
    statsSource = decodeURIComponent(id);
    loadStats();
  }
  window.scrollTo(0, 0);
}

// ---------- eventi ----------

$('new-site-form').addEventListener('submit', event => {
  event.preventDefault();
  perform(event.submitter, async () => {
    const {site} = await api('site-add', {name: $('new-site-name').value.trim()});
    await refresh(true);
    location.hash = '#site/' + site;
  });
});

$('collect').addEventListener('click', () =>
  perform($('collect'), async () => {
    const data = await api('collect');
    showNotice(data.message);
    await refresh();
  }),
);

$('search-form').addEventListener('submit', (event) => {
  event.preventDefault();
  hideNotice();
  perform(event.submitter, () => search());
});

$('clear-search').addEventListener('click', clearSearch);
$('search-source').addEventListener('change', () => {
  renderLogStatus();
  search();
});
$('live').addEventListener('change', () => {
  if ($('live').checked) search();
  else stopLive();
});

// ---------- impostazioni della sorgente ----------

const PERIOD_WORDS = { days: ['giorno', 'giorni'], months: ['mese', 'mesi'], years: ['anno', 'anni'] };
const PERIOD_MAX = { days: 36500, months: 1200, years: 100 };

function sourcePeriod(source) {
  return source.retention;
}

function periodText(period) {
  return period.amount + ' ' + PERIOD_WORDS[period.unit][period.amount === 1 ? 0 : 1];
}

function readPeriod(amountId, unitId) {
  const amount = Number($(amountId).value);
  const unit = $(unitId).value;
  if (!Number.isInteger(amount) || amount < 1 || amount > PERIOD_MAX[unit]) {
    throw new Error('Scegli una durata valida (fino a 100 anni).');
  }
  return { amount, unit };
}

function requiredFolder(id) {
  const folder = $(id).value.trim();
  if (!folder) {
    throw new Error('Indica la cartella personalizzata oppure scegli quella predefinita.');
  }
  return folder;
}

function updateStorage(prefix) {
  $(prefix + '-custom-storage').hidden = $(prefix + '-storage').value !== 'custom';
}

function populateSettings() {
  const s = settingsSource;
  const period = sourcePeriod(s);
  $('settings-owner').replaceChildren(document.createTextNode('Gruppo · ' + (state.sites[s.site]?.name || 'Log da sistemare') + ' '), assignLogButton(s));
  const isPodman = s.input.kind === 'podman';
  const hasFiles = s.input.kind === 'file' || s.input.kind === 'folder';

  $('settings-origin-panel').hidden = hasFiles;
  if (!hasFiles) $('settings-original-retention').replaceChildren(originalRetentionNote(s.input, s));
  $('settings-label').value = s.label || s.source;
  $('settings-kind').value = s.log_kind || 'application';
  $('retention-days').disabled = $('retention-unit').disabled = !!s.class;
  $('retention-class-note').textContent = s.class ? 'Durata definita dalla classe '+s.class+' in Regole e siti.' : '';
  $('retention-days').value = period.amount;
  $('retention-unit').value = period.unit;

  $('settings-storage').value = s.archive_dir ? 'custom' : 'default';
  $('settings-archive-dir').value = s.archive_dir || '';
  $('settings-default-path').textContent = state.archive_directory + '/archives/' + s.source + '.db';
  updateStorage('settings');

  $('file-source-settings').hidden = !hasFiles;
  $('podman-reconnect').hidden = !isPodman;
  showContainerAdvice(s);
  $('settings-source-path').value = s.input.path || '';
  $('cleanup-panel').hidden = !hasFiles;
  $('rotated-settings').hidden = $('rotated-choice').hidden = s.input.kind !== 'file';
  $('legacy-choice').hidden = !(s.remove_rotated && s.completed_dir);
  $('rotated-pattern').value = s.input.rotated_pattern || '';
  $('rotated-pattern').dataset.edited = '';
  const mode = s.completed_dir ? (s.remove_rotated ? 'both' : 'handoff') : (s.remove_rotated ? 'rotated' : 'keep');
  document.querySelector('input[name=cleanup-mode][value=' + mode + ']').checked = true;
  $('completed-dir').value = s.completed_dir || '';
  $('handoff-confirmed').checked = false;
  $('cleanup-settings').hidden = !s.completed_dir;
  if (hasFiles) refreshCleanup();

  updateLifecycle(s);
  $('replacement-container').replaceChildren(new Option('Carica prima l’elenco', ''));
  $('reconnect-connection').value = '';
  loadAdvice(s.source, $('settings-advice'), false);
}

function updateLifecycle(s) {
  $('lifecycle-status').textContent = 'Stato · ' + sourceBadge(s)[0];
  const paused = !s.enabled || s.detached;
  $('pause-source').textContent = paused ? 'Riprendi la raccolta' : 'Metti in pausa';
  $('detach-source').disabled = s.detached;
  $('remove-source').disabled = !paused;
  $('last-copy').hidden = !s.deleted_originals;
  $('settings-gaps').replaceChildren(...s.gaps.slice(0, 3).map(g =>
    element('p', 'Potrebbero mancare eventi dal ' + formatDate(g.from) + ' al ' + formatDate(g.to) + '. ' + g.reason, 'card-warning')));
}

// Per i container: dove il runtime tiene i log e come limitarli.
async function showContainerAdvice(source) {
  const box = $('container-log-box');
  const runtime = source.input.kind;
  box.hidden = true;
  if (runtime !== 'podman' && runtime !== 'docker') return;
  try {
    const connection = source.input.connection || source.input.context || null;
    const logs = await api('container-logs', { runtime, connection, container: source.input.container });
    $('container-log-advice').textContent = 'Driver dei log: ' + logs.driver + '. ' + logs.advice;
    box.hidden = false;
  } catch (_) {
    // Il container potrebbe non esistere più: il resto delle impostazioni resta utilizzabile.
  }
}


// Raccoglie dal modulo solo ciò che serve: il percorso parte solo se è stato cambiato,
// così una sorgente con la cartella sparita resta modificabile.
function settingsChange() {
  const s = settingsSource;
  const customStorage = $('settings-storage').value === 'custom';
  const change = {
    name: s.source,
    label: $('settings-label').value.trim(),
    retention: readPeriod('retention-days', 'retention-unit'),
    log_kind: $('settings-kind').value,
    archive_dir: customStorage ? requiredFolder('settings-archive-dir') : '',
  };
  if (s.input.kind !== 'file' && s.input.kind !== 'folder') return change;

  const path = $('settings-source-path').value.trim();
  if (path && path !== s.input.path) change.path = path;
  if (s.input.kind === 'file') {
    const pattern = $('rotated-pattern').value.trim();
    if (cleanRotated() && !pattern) {
      throw new Error('Apri Avanzate e indica come riconoscere i vecchi file.');
    }
    const usePattern = s.input.rotated_pattern || cleanRotated() || $('rotated-pattern').dataset.edited;
    if (usePattern && pattern !== (s.input.rotated_pattern || '')) change.rotated_pattern = pattern;
    change.remove_rotated = cleanRotated() && pattern !== '';
  }
  const handoff = cleanFolder();
  change.completed_dir = handoff ? requiredFolder('completed-dir') : '';
  change.handoff_confirmed = $('handoff-confirmed').checked;
  return change;
}

$('retention-form').addEventListener('submit', (event) => {
  event.preventDefault();
  perform(event.submitter, async () => {
    if (!settingsSource) return;
    const change = settingsChange();
    const before = sourcePeriod(settingsSource);
    const changed =
      change.retention.amount !== before.amount || change.retention.unit !== before.unit;
    const question =
      'Impostare ' + periodText(change.retention) + '? ' +
      'Alla prossima lettura le copie oltre questa durata saranno eliminate.';
    if (changed && !confirm(question)) return;
    const warning = cleanupTurnedOn(change);
    if (warning && !confirm(warning)) return;
    await api('source-settings', change);
    await refresh(true);
    goToSources('Impostazioni salvate.');
  });
});

$('settings-storage').addEventListener('change', () => updateStorage('settings'));
const cleanupMode = () => document.querySelector('input[name=cleanup-mode]:checked')?.value;
const cleanRotated = () => ['rotated', 'both'].includes(cleanupMode());
const cleanFolder = () => ['handoff', 'both'].includes(cleanupMode());
let cleanupTimer;
for (const option of document.querySelectorAll('input[name=cleanup-mode]')) {
  option.onchange = () => {
    $('cleanup-settings').hidden = !cleanFolder();
    refreshCleanup();
  };
}
$('rotated-pattern').oninput = () => {
  $('rotated-pattern').dataset.edited = 'yes';
  clearTimeout(cleanupTimer);
  cleanupTimer = setTimeout(refreshCleanup, 250);
};
$('use-suggested-pattern').onclick = () => {
  $('rotated-pattern').value = $('use-suggested-pattern').dataset.pattern;
  $('rotated-pattern').oninput();
};
for (const id of ['completed-dir','retention-days','retention-unit']) $(id).addEventListener('input', updateCleanupWarning);
function cleanupWarningText(rotated, handoff) {
  const parts = [];
  if (rotated) parts.push('i vecchi file accanto a ' + ($('settings-source-path').value || settingsSource.input.path).split('/').pop());
  const folder = $('completed-dir').value.trim();
  if (handoff && !folder) return 'Scegli la cartella dei log chiusi prima di salvare.';
  if (handoff) parts.push('i file .log in «' + folder + '»');
  const period = {amount: Number($('retention-days').value), unit: $('retention-unit').value};
  return 'Salvando questa scelta, Loglem potrà cancellare ' + parts.join(' e ') +
    ' dopo aver verificato la copia e atteso 24 ore dall’ultima modifica. Il file in uso resta al suo posto.' +
    (period.amount > 0 ? ' La copia in Loglem scade dopo ' + periodText(period) + '.' : '');
}
function cleanupTurnedOn(change) {
  const rotated = change.remove_rotated && (!settingsSource.remove_rotated || change.rotated_pattern !== undefined || change.path);
  const handoff = change.completed_dir && change.completed_dir !== settingsSource.completed_dir;
  return rotated || handoff ? cleanupWarningText(rotated, handoff) : '';
}
function updateCleanupWarning() {
  const warning = $('cleanup-warning');
  warning.hidden = !(cleanRotated() || cleanFolder());
  if (!warning.hidden) warning.textContent = cleanupWarningText(cleanRotated(), cleanFolder());
}
let cleanupRequest = 0;
async function refreshCleanup() {
  const s = settingsSource;
  if (!s || !['file','folder'].includes(s.input.kind)) return;
  const request = ++cleanupRequest;
  updateCleanupWarning();
  const field = $('rotated-pattern');
  const body = {name:s.source, rotated:cleanRotated(), handoff:cleanFolder()};
  if (s.input.kind === 'file' && (field.value || field.dataset.edited)) body.pattern = field.value.trim();
  try {
    const r = await api('cleanup', body);
    if (s !== settingsSource || request !== cleanupRequest) return;
    const other = ['logrotate','program'].includes(r.system.kind);
    $('cleanup-headline').textContent = 'Oggi Loglem ' + (s.remove_rotated || s.completed_dir ? 'può cancellare i vecchi file.' : 'conserva gli originali.') +
      (other ? ' Gestione del programma: ' + r.system.text + '.' : ' La durata degli originali è sconosciuta.');
    $('cleanup-notes').replaceChildren(...r.notes.filter(n => n.level === 'warn').map(n => element('li', n.text, 'cleanup-warn')));
    if (s.input.kind !== 'file') return;
    if (!field.value && !field.dataset.edited) {
      field.value = r.suggested_pattern || s.input.path.split('/').pop() + '.*';
      return refreshCleanup();
    }
    const button = $('use-suggested-pattern');
    button.hidden = !r.suggested_pattern || r.suggested_pattern === field.value;
    button.dataset.pattern = r.suggested_pattern || '';
    button.textContent = 'Usa ' + r.suggested_pattern;
    $('matched-files').textContent = r.matching_files === null ? 'Impossibile leggere la cartella.' :
      r.matching_files.length ? 'File trovati (fino a 12): ' + r.matching_files.join(', ') : 'Nessun vecchio file corrisponde al nome indicato.';
  } catch (error) {
    if (s === settingsSource && request === cleanupRequest) $('matched-files').textContent = error.message;
  }
}
$('pause-source').onclick = () => perform($('pause-source'), async () => {
  const s = settingsSource;
  if (s.enabled && !s.detached) {
    $('schedule-pause').dataset.source = s.source;
    $('pause-duration').value = '60';
    $('pause-policy').textContent = 'Durata degli originali sconosciuta. Il recupero dipende dai log ancora disponibili.';
    $('pause-dialog').showModal();
    if (['folder','file'].includes(s.input.kind)) {
      const r = await api('cleanup', {name:s.source});
      if (s === settingsSource && r.system.days) $('pause-policy').textContent = 'Durata stimata degli originali: circa ' + r.system.days + ' giorni.';
    } else {
      const policy = await api('original-retention', {name:s.source});
      if (s === settingsSource) $('pause-policy').textContent = policy;
    }
    return;
  }
  await api('source-settings', {name:s.source, enabled:true});
  await refresh();
  location.hash = '#sources';
});
$('schedule-pause').onsubmit = event => {
  event.preventDefault();
  perform(event.submitter, async () => {
    const minutes = Number($('pause-duration').value);
    await api('source-settings', {name:$('schedule-pause').dataset.source, enabled:false, pause_minutes:minutes || null});
    $('pause-dialog').close();
    await refresh();
    location.hash = '#sources';
  });
};

$('detach-source').addEventListener('click', () =>
  perform($('detach-source'), async () => {
    const question =
      'Scollegare questa sorgente? Lo storico resta disponibile e le copie scadranno ' +
      'secondo la durata scelta.';
    if (!confirm(question)) return;
    await api('source-settings', { name: settingsSource.source, detached: true });
    await refresh();
    location.hash = '#sources';
  }),
);

$('remove-source').onclick = () => {
  $('remove-summary').textContent = 'Stai per eliminare «' + settingsSource.label + '» e il suo storico da Loglem. Gli originali ancora presenti restano al loro posto.';
  $('last-copy').hidden = !settingsSource.deleted_originals;
  $('remove-dialog').showModal();
};
for (const id of ['remove-backup','remove-direct']) $(id).onclick = () => perform($(id), async () => {
  const backup = id === 'remove-backup';
  if (!backup && !confirm('Eliminare definitivamente questo storico senza creare un backup?')) return;
  const dialog = $('remove-dialog');
  const controls = [...dialog.querySelectorAll('button')];
  controls.forEach(b => b.disabled = true);
  try {
    const result = await api('source-remove', {name:settingsSource.source, backup});
    dialog.close();
    await refresh(true);
    goToSources(result.backup ? 'Storico eliminato. Backup verificato in ' + result.backup : result.message);
  } finally {
    controls.forEach(b => b.disabled = sessionLocked);
  }
});

$('load-replacements').addEventListener('click', () =>
  perform($('load-replacements'), async () => {
    const connection =
      $('reconnect-connection').value.trim() || settingsSource.input.connection || null;
    const data = await api('podman', { connection });
    const select = $('replacement-container');
    select.replaceChildren(new Option('Scegli il sostituto', ''));
    for (const c of data.containers) {
      select.add(new Option(c.name + ' · ' + c.id.slice(0, 12), c.id));
    }
  }),
);

$('reconnect-source').addEventListener('click', () =>
  perform($('reconnect-source'), async () => {
    const container = $('replacement-container').value;
    if (!container) throw new Error('Scegli il container sostitutivo.');
    await api('source-settings', {
      name: settingsSource.source,
      container,
      connection: $('reconnect-connection').value.trim(),
      enabled: true,
    });
    await refresh();
    location.hash = '#logs/' + encodeURIComponent(settingsSource.source);
  }),
);

// ---------- finestra «scegli una cartella» ----------

let storageTarget = null; // id del campo da riempire
let storageFolder = null; // cartella mostrata nella finestra

async function browseStorage(path = '') {
  try {
    storageFolder = await api('folders', { path });
    $('storage-browser-path').value = storageFolder.path;
    $('storage-browser-error').textContent = '';
    $('storage-browser-parent').disabled = !storageFolder.parent;
    const items = $('storage-browser-items');
    items.replaceChildren();
    for (const subfolder of storageFolder.folders) {
      const button = element('button', '▱ ' + subfolder.split('/').pop());
      button.type = 'button';
      button.addEventListener('click', () => browseStorage(subfolder));
      items.append(button);
    }
  } catch (error) {
    $('storage-browser-error').textContent = error.message;
  } finally {
    $('storage-browser-use').disabled = !storageFolder;
  }
}

for (const button of document.querySelectorAll('[data-storage-for]')) {
  button.addEventListener('click', () => {
    storageTarget = button.dataset.storageFor;
    storageFolder = null;
    $('storage-dialog').showModal();
    browseStorage($(storageTarget).value || '');
  });
}

$('storage-browser-open').addEventListener('click', () =>
  browseStorage($('storage-browser-path').value),
);
$('storage-browser-parent').addEventListener('click', () => browseStorage(storageFolder.parent));
$('storage-browser-use').addEventListener('click', () => {
  if (!storageFolder) return;
  $(storageTarget).value = storageFolder.path;
  $('storage-dialog').close();
});
$('storage-browser-cancel').addEventListener('click', () => $('storage-dialog').close());

$('reload-diagnostics').addEventListener('click', () =>
  perform($('reload-diagnostics'), loadDiagnostics),
);

window.addEventListener('hashchange', route);

// Mostra chi è collegato; chi può solo consultare non vede i comandi di modifica.
async function loadUser() {
  const me = await api('me');
  if (me.role === 'reader') document.body.classList.add('reader');
  if (!me.login) return;
  $('user-name').textContent = me.user + (me.role === 'reader' ? ' · sola lettura' : '');
  $('user-box').hidden = false;
}

$('logout').addEventListener('click', async () => {
  await api('logout').catch(() => {});
  location.reload();
});

window.addEventListener('DOMContentLoaded', async () => {
  await loadUser().catch(() => {});
  await refresh();
  route();
  setInterval(() => refresh('poll'), REFRESH_MS);
});
