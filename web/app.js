'use strict';

const $ = (id) => document.getElementById(id);
const token = document.querySelector('meta[name="loglem-token"]').content;

const REFRESH_MS = 2000;
const PAGES = {
  sources: ['Le tue sorgenti', 'Qui vedi da dove Loglem legge i log e quanto spazio occupano.'],
  logs: ['Esplora i log', 'Scegli una sorgente, cerca una parola e filtra per data.'],
  add: ['Aggiungi una sorgente', 'Tre passaggi per iniziare a salvare i log.'],
  settings: ['Impostazioni sorgente', 'Nome, durata, posizione dei dati e collegamento.'],
  diagnostics: ['Problemi', 'Se qualcosa non funziona, qui trovi cosa è successo.'],
};

const SOURCE_KINDS = { podman: 'PODMAN', journal: 'JOURNAL' };

let state = { sources: [], collection: {} };
let view = 'sources';
let selected = null; // sorgente in corso di aggiunta
let folder = null;
let container = null;
let addKind = null;
let settingsSource = null;
let refreshing = false;
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

function hideNotice() {
  $('notice').hidden = true;
}

async function api(path, data = {}) {
  let response;
  try {
    response = await fetch('/api/' + path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Loglem-Token': token },
      body: JSON.stringify(data),
    });
  } catch (_) {
    throw new Error('Loglem non risponde. Controlla che sia ancora aperto nel terminale.');
  }
  if (response.status === 401) {
    // Sessione scaduta o utente eliminato: la pagina mostra di nuovo l'accesso.
    location.reload();
    throw new Error('Accesso scaduto: entra di nuovo.');
  }
  const type = response.headers.get('content-type') || '';
  if (!type.includes('application/json')) {
    throw new Error(
      'La sessione è scaduta. Ricarica la pagina usando l’indirizzo mostrato da Loglem.',
    );
  }
  const result = await response.json();
  if (!response.ok) {
    throw new Error(result.error || 'Operazione non riuscita. Apri «Problemi» per i dettagli.');
  }
  return result;
}

// Disabilita il pulsante mentre l'azione è in corso e mostra eventuali errori.
async function perform(button, action) {
  const label = button.textContent;
  button.disabled = true;
  button.textContent = 'Attendi…';
  try {
    await action();
  } catch (error) {
    showNotice(error.message, true);
  } finally {
    button.disabled = false;
    button.textContent = label;
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
  if (sourceError(source)) return ['Errore di lettura', 'error'];
  if (!source.enabled) return ['In pausa', 'waiting'];
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

function renderEmptySources(list) {
  const empty = element('div', undefined, 'empty-state');
  empty.append(
    element('div', '▱', 'empty-icon'),
    element('h2', 'Inizia collegando una sorgente'),
    element(
      'p',
      'Una sorgente è una cartella con file .log oppure un container Podman. ' +
        'Loglem ne salva una copia in cui puoi cercare. I tuoi file originali non vengono toccati.',
    ),
    link('＋ Aggiungi la prima sorgente', '#add', 'button primary admin-only'),
  );
  list.append(empty);
}

function renderSourceCard(source) {
  const card = element('article', undefined, 'source-card');

  const heading = element('div', undefined, 'card-heading');
  const title = element('div');
  title.append(
    element('span', SOURCE_KINDS[source.input.kind] || 'FILE DI LOG', 'source-kind'),
    element('h3', source.label || source.source),
  );
  const [badgeText, badgeClass] = sourceBadge(source);
  heading.append(title, element('span', badgeText, 'badge ' + badgeClass));
  card.append(heading, element('p', source.origin, 'source-origin'));

  const metrics = element('div', undefined, 'card-metrics');
  const values = [
    [source.entries.toLocaleString('it-IT'), 'righe salvate'],
    [periodText(sourcePeriod(source)), 'di conservazione'],
  ];
  for (const [value, label] of values) {
    const metric = element('div');
    metric.append(element('strong', value), element('span', label));
    metrics.append(metric);
  }
  card.append(metrics);

  const error = sourceError(source);
  if (source.warning) card.append(element('p', source.warning, 'card-warning'));
  if (error) {
    card.append(element('p', error, 'card-error'));
  } else if (source.entries === 0) {
    card.append(
      element(
        'small',
        source.last_collection
          ? 'Per ora la sorgente non ha prodotto righe nel periodo conservato.'
          : 'In attesa della prima lettura…',
      ),
    );
  }

  const actions = element('div', undefined, 'row');
  actions.append(
    link('Vedi i log →', '#logs/' + encodeURIComponent(source.source), 'button primary'),
    link('Impostazioni', '#settings/' + encodeURIComponent(source.source), 'button admin-only'),
  );
  card.append(actions);

  const details = element('details');
  details.append(
    element('summary', 'Dove sono i dati'),
    element('p', 'Copia salvata in: ' + source.archive, 'path'),
    element(
      'p',
      'Spazio: ' + formatSize(source.archive_bytes) + ' · Primo log: ' + formatDate(source.earliest),
      'path',
    ),
    element('p', source.message, 'muted'),
  );
  card.append(details);
  return card;
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
  const signature = JSON.stringify(sources);
  if (signature === lastSignature) return;
  lastSignature = signature;

  const list = $('source-list');
  list.replaceChildren();
  if (sources.length === 0) {
    renderEmptySources(list);
    return;
  }
  for (const source of sources) list.append(renderSourceCard(source));
}

function sourceOptionLabel(source) {
  return (source.label || source.source) + (source.detached ? ' (storico)' : '');
}

function syncSourceSelect() {
  const select = $('search-source');
  const previous = select.value;
  // Ricostruisce l'elenco solo se nomi o etichette sono cambiati.
  const wanted = JSON.stringify(state.sources.map((s) => [s.source, sourceOptionLabel(s)]));
  const current = JSON.stringify([...select.options].slice(1).map((o) => [o.value, o.textContent]));
  if (wanted === current) return;

  select.replaceChildren(new Option('Tutte le sorgenti', ''));
  for (const source of state.sources) {
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
  if (state.sources.some((s) => s.source === chosen)) select.value = chosen;
}

async function refresh() {
  if (refreshing) return;
  refreshing = true;
  try {
    state = await api('status');
    $('version').textContent = 'Versione ' + state.version;
    $('data-dir').textContent = state.archive_directory;
    $('connection-status').textContent = 'Collegato';
    $('footer-status').textContent = 'in funzione';
    syncSourceSelect();
    renderSources();
    if (view === 'logs') {
      renderLogStatus();
    }
  } catch (error) {
    $('connection-status').textContent = 'Non collegato';
    $('footer-status').textContent = error.message;
    if (view === 'sources' || view === 'logs') showNotice(error.message, true);
  } finally {
    refreshing = false;
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

function renderLogRow(row, query) {
  const line = element('div', undefined, 'log-row');
  const stamp = element('div', formatDate(row.timestamp), 'log-date');
  if (row.timestamp_origin !== 'event') {
    stamp.append(element('small', 'Data di lettura (il log non indica la sua data)'));
  }
  const message = element('pre', undefined, 'log-text');
  highlight(message, row.message, query);
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
    const { rows } = await api('search', {
      source: $('search-source').value,
      text: query,
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
    if (rows.length === 0) renderNoResults(list);
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
  return view === 'logs' && $('live').checked && !$('search-to').value;
}

function stopLive() {
  liveRun++;
}

// Chiede al server le righe nuove (ogni richiesta aspetta fino a 15 secondi)
// e le aggiunge in cima, con gli stessi filtri della ricerca.
async function startLive() {
  const run = ++liveRun;
  if (!liveWanted()) return;
  const filters = { source: $('search-source').value, text: $('search-text').value };
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

function clearSearch() {
  $('search-text').value = '';
  $('search-from').value = '';
  $('search-to').value = '';
  search();
}

// ---------- aggiunta di una sorgente ----------

function showWizardStep(step) {
  for (const item of document.querySelectorAll('[data-step]')) {
    item.classList.toggle('current', Number(item.dataset.step) === step);
  }
  $('add-type').hidden = step !== 1;
  $('folder-step').hidden = !(step === 2 && (addKind === 'folder' || addKind === 'caddy'));
  $('podman-step').hidden = !(step === 2 && addKind === 'podman');
  $('journal-step').hidden = !(step === 2 && addKind === 'journal');
  $('save-source').hidden = step !== 3;
}

async function openFolder(path = '') {
  folder = await api('folders', { path });
  $('folder-path').value = folder.path;
  $('folder-parent').disabled = !folder.parent;

  const items = $('folder-items');
  items.replaceChildren();
  for (const subfolder of folder.folders) {
    const button = element('button', '▱  ' + subfolder.split('/').pop());
    button.type = 'button';
    button.addEventListener('click', () => perform(button, () => openFolder(subfolder)));
    items.append(button);
  }
  if (folder.folders.length === 0) items.append(element('p', 'Nessuna sottocartella.', 'muted'));

  const count = folder.logs.length;
  $('log-count').textContent =
    count === 0
      ? 'Nessun file .log in questa cartella'
      : count === 1
        ? '1 file .log trovato'
        : count + ' file .log trovati';
  $('log-files').replaceChildren(...folder.logs.map(logFileItem));
}

// Per Caddy ogni file .log è un pulsante: si sceglie il file attivo, non la cartella.
function logFileItem(file) {
  const item = element('li');
  const name = file.split('/').pop();
  if (addKind !== 'caddy') {
    item.textContent = name;
    return item;
  }
  const button = element('button', name + ' — usa questo file →');
  button.type = 'button';
  button.addEventListener('click', () => {
    const stem = name.replace(/\.log$/, '');
    configureSource({ kind: 'caddy', path: file }, 'Log di Caddy · ' + file, 'caddy-' + stem);
    $('source-kind').value = 'access';
  });
  item.append(button);
  return item;
}

// Adatta il passo «cartella» al tipo scelto (cartella intera o log di Caddy).
function prepareFolderStep() {
  const caddy = addKind === 'caddy';
  $('folder-step-title').textContent = caddy
    ? 'Dov\'è il log di Caddy?'
    : 'Quale cartella vuoi seguire?';
  $('folder-step-help').textContent = caddy
    ? 'Apri la cartella dei log di Caddy (spesso /var/log/caddy) e scegli il file su cui ' +
      'scrive, di solito access.log.'
    : 'Naviga tra le cartelle del computer su cui gira Loglem, oppure scrivi il percorso e ' +
      'premi Invio.';
  $('folder-preview-help').textContent = caddy
    ? 'I file ruotati da Caddy (es. access-2026-11-16T10-00-00.000.log) vengono letti in automatico.'
    : 'Vengono letti i file .log di questa cartella, anche quelli futuri. Le sottocartelle sono escluse.';
  $('use-folder').hidden = caddy;
}

function containerChoice(c, connection) {
  const label = element('label', undefined, 'container-choice');
  const radio = element('input');
  radio.type = 'radio';
  radio.name = 'container';
  radio.value = c.id;
  radio.addEventListener('change', () => {
    container = { ...c, connection };
    $('use-podman').disabled = false;
  });
  const text = element('div');
  const details =
    (c.pod ? 'Pod ' + c.pod + ' · ' : '') + (c.state || 'stato sconosciuto') + ' · ' + c.id.slice(0, 12);
  text.append(element('strong', c.name || c.id.slice(0, 12)), element('small', details));
  label.append(radio, text);
  return label;
}

async function loadContainers() {
  container = null;
  $('use-podman').disabled = true;
  const list = $('container-list');
  list.replaceChildren(element('p', 'Cerco i container…', 'muted'));
  try {
    const connection = $('connection').value.trim() || null;
    const data = await api('podman', { connection });
    list.replaceChildren();
    if (data.containers.length === 0) {
      list.append(
        element(
          'p',
          'Non trovo nessun container. Controlla che Podman sia installato e che i container ' +
            'siano stati creati dallo stesso utente che ha avviato Loglem.',
          'muted',
        ),
      );
      return;
    }
    for (const c of data.containers) list.append(containerChoice(c, connection));
  } catch (error) {
    list.replaceChildren(element('div', error.message, 'card-error'));
    throw error;
  }
}

async function loadJournalUnits() {
  const list = $('journal-units');
  list.replaceChildren(element('p', 'Leggo i servizi presenti nel journal…', 'muted'));
  try {
    const { units } = await api('journal-units');
    list.replaceChildren();
    if (units.length === 0) {
      list.append(element('p', 'Nessun servizio trovato: verrà letto tutto il journal.', 'muted'));
    }
    for (const unit of units) {
      const label = element('label', undefined, 'container-choice');
      const box = element('input');
      box.type = 'checkbox';
      box.value = unit;
      label.append(box, element('strong', unit));
      list.append(label);
    }
  } catch (error) {
    list.replaceChildren(element('div', error.message, 'card-error'));
    throw error;
  }
}

function journalFilter() {
  const units = [...document.querySelectorAll('#journal-units input:checked')].map((b) => b.value);
  const identifiers = $('journal-identifiers')
    .value.split(',')
    .map((s) => s.trim())
    .filter(Boolean);
  const priority = $('journal-priority').value;
  return { units, identifiers, priority: priority === '' ? null : Number(priority) };
}

function describeJournal(filter) {
  const parts = [];
  if (filter.units.length) parts.push('servizi ' + filter.units.join(', '));
  if (filter.identifiers.length) parts.push('programmi ' + filter.identifiers.join(', '));
  return parts.length ? parts.join(' · ') : 'tutto il journal';
}

// Passa all'ultimo passaggio proponendo un nome libero.
function configureSource(source, description, suggestion) {
  selected = source;
  $('chosen-source').textContent = description;
  const base = suggestion.replace(/[^a-zA-Z0-9_-]/g, '-').slice(0, 48) || 'logs';
  let name = base;
  let n = 2;
  while (state.sources.some((s) => s.source === name)) name = base + '-' + n++;
  $('source-name').value = name;
  $('source-days').value = 28;
  $('source-unit').value = 'days';
  $('source-kind').value = 'generic';
  $('source-storage').value = 'default';
  $('source-archive-dir').value = '';
  updateStorage('source');
  $('archive-location').textContent = state.archive_directory + '/archives/' + name + '.db';
  showWizardStep(3);
  $('source-name').focus();
}

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
  const [requested, id = ''] = location.hash.slice(1).split('/');
  view = Object.hasOwn(PAGES, requested) ? requested : 'sources';

  for (const section of document.querySelectorAll('[data-view]')) {
    section.hidden = section.dataset.view !== view;
  }
  const activeNav = view === 'settings' ? 'sources' : view;
  for (const item of document.querySelectorAll('[data-nav]')) {
    if (item.dataset.nav === activeNav) item.setAttribute('aria-current', 'page');
    else item.removeAttribute('aria-current');
  }

  const [title, subtitle] = PAGES[view];
  $('page-title').textContent = title;
  $('page-subtitle').textContent = subtitle;
  $('breadcrumb').textContent = title.toUpperCase();
  $('header-add').hidden = view === 'add';
  document.title = 'Loglem · ' + title;
  hideNotice();
  stopLive();

  if (view === 'logs') {
    try {
      $('search-source').value = id ? decodeURIComponent(id) : '';
    } catch (_) {
      $('search-source').value = '';
    }
    renderLogStatus();
    search();
  } else if (view === 'sources') {
    lastSignature = '';
    renderSources();
  } else if (view === 'add') {
    selected = null;
    container = null;
    showWizardStep(1);
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
    if (settingsSource) populateSettings();
  } else if (view === 'diagnostics') {
    loadDiagnostics();
  }
  window.scrollTo(0, 0);
}

// ---------- eventi ----------

for (const kind of ['folder', 'caddy']) {
  const button = $('choose-' + kind);
  button.addEventListener('click', () =>
    perform(button, async () => {
      addKind = kind;
      prepareFolderStep();
      showWizardStep(2);
      await openFolder();
    }),
  );
}

$('choose-podman').addEventListener('click', () =>
  perform($('choose-podman'), async () => {
    addKind = 'podman';
    showWizardStep(2);
    await loadContainers();
  }),
);

for (const button of document.querySelectorAll('[data-back-type]')) {
  button.addEventListener('click', () => {
    hideNotice();
    showWizardStep(1);
  });
}

$('open-folder').addEventListener('click', () =>
  perform($('open-folder'), () => openFolder($('folder-path').value)),
);

$('folder-path').addEventListener('keydown', (event) => {
  if (event.key === 'Enter') {
    event.preventDefault();
    $('open-folder').click();
  }
});

$('folder-parent').addEventListener('click', () =>
  perform($('folder-parent'), () => openFolder(folder.parent)),
);

$('use-folder').addEventListener('click', () => {
  if (!folder) return;
  const name = folder.path.split('/').pop() || 'logs';
  configureSource({ kind: 'folder', path: folder.path }, 'Cartella · ' + folder.path, name);
});

$('load-podman').addEventListener('click', () => perform($('load-podman'), loadContainers));

$('choose-journal').addEventListener('click', () =>
  perform($('choose-journal'), async () => {
    addKind = 'journal';
    showWizardStep(2);
    await loadJournalUnits();
  }),
);

$('use-journal').addEventListener('click', () => {
  const filter = journalFilter();
  const suggestion = filter.units.length === 1 ? filter.units[0].replace(/\.service$/, '') : 'journal';
  configureSource({ kind: 'journal', journal: filter }, 'Journal · ' + describeJournal(filter), suggestion);
});

$('use-podman').addEventListener('click', () => {
  if (!container) return;
  configureSource(
    { kind: 'podman', container: container.id, connection: container.connection },
    'Container Podman · ' + container.name,
    container.name,
  );
});

$('save-source').addEventListener('submit', (event) => {
  event.preventDefault();
  perform(event.submitter, async () => {
    if (!selected) return;
    const name = $('source-name').value.trim();
    const customStorage = $('source-storage').value === 'custom';
    const result = await api('add', {
      ...selected,
      name,
      retention: readPeriod('source-days', 'source-unit'),
      log_kind: $('source-kind').value,
      archive_dir: customStorage ? requiredFolder('source-archive-dir') : null,
    });
    await refresh();
    selected = null;
    location.hash = '#logs/' + encodeURIComponent(name);
    showNotice('Sorgente salvata. Sto leggendo i log: le righe compariranno qui da sole.');
    if (result.snippet) showCaddyDialog(result.snippet);
  });
});

$('cancel-add').addEventListener('click', () => {
  selected = null;
  location.hash = '#sources';
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
  return source.retention || { amount: source.retention_days, unit: 'days' };
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
  const isPodman = s.input.kind === 'podman';
  const hasFiles = s.input.kind === 'file' || s.input.kind === 'folder';
  const isRotatedFile = s.input.kind === 'file' && Boolean(s.input.rotated_pattern);

  $('settings-label').value = s.label || s.source;
  $('settings-kind').value = s.log_kind || 'generic';
  $('retention-days').value = period.amount;
  $('retention-unit').value = period.unit;

  $('settings-storage').value = s.archive_dir ? 'custom' : 'default';
  $('settings-archive-dir').value = s.archive_dir || '';
  $('settings-default-path').textContent = state.archive_directory + '/archives/' + s.source + '.db';
  updateStorage('settings');

  $('file-source-settings').hidden = !hasFiles;
  $('podman-reconnect').hidden = !isPodman;
  $('settings-source-path').value = s.input.path || '';
  $('rotated-settings').hidden = !isRotatedFile;
  $('remove-rotated').checked = s.remove_rotated;
  $('caddy-snippet-box').hidden = !s.remove_rotated;
  if (s.remove_rotated) loadCaddySnippet(s.input.path);
  $('cleanup-mode').value = s.completed_dir ? 'handoff' : 'keep';
  $('completed-dir').value = s.completed_dir || '';
  $('handoff-confirmed').checked = false;
  $('cleanup-settings').hidden = !s.completed_dir;

  const paused = !s.enabled || s.detached;
  $('pause-source').textContent = paused ? 'Riprendi la raccolta' : 'Metti in pausa';
  $('detach-source').disabled = s.detached;
  $('replacement-container').replaceChildren(new Option('Carica prima l’elenco', ''));
  $('reconnect-connection').value = '';
}

async function loadCaddySnippet(path) {
  try {
    const { snippet } = await api('caddy-snippet', { path });
    $('caddy-snippet').textContent = snippet;
  } catch (_) {
    $('caddy-snippet-box').hidden = true;
  }
}

function showCaddyDialog(snippet) {
  $('caddy-dialog-snippet').textContent = snippet;
  $('caddy-dialog').showModal();
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
  if (s.input.kind === 'file' && s.input.rotated_pattern) {
    change.remove_rotated = $('remove-rotated').checked;
  }
  const handoff = $('cleanup-mode').value === 'handoff';
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
    await api('source-settings', change);
    await refresh();
    location.hash = '#sources';
    showNotice('Impostazioni salvate. La conservazione sarà applicata automaticamente.');
  });
});

$('source-storage').addEventListener('change', () => updateStorage('source'));
$('settings-storage').addEventListener('change', () => updateStorage('settings'));
$('cleanup-mode').addEventListener('change', () => {
  $('cleanup-settings').hidden = $('cleanup-mode').value !== 'handoff';
});

$('pause-source').addEventListener('click', () =>
  perform($('pause-source'), async () => {
    const resume = !settingsSource.enabled || settingsSource.detached;
    await api('source-settings', { name: settingsSource.source, enabled: resume });
    await refresh();
    location.hash = '#sources';
  }),
);

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

// ---------- finestra Caddy ----------

$('caddy-dialog-close').addEventListener('click', () => $('caddy-dialog').close());
$('caddy-dialog-copy').addEventListener('click', async () => {
  try {
    await navigator.clipboard.writeText($('caddy-dialog-snippet').textContent);
    $('caddy-dialog-copy').textContent = 'Copiato ✓';
  } catch (_) {
    $('caddy-dialog-copy').textContent = 'Selezionalo e copialo a mano';
  }
});

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

(async () => {
  await loadUser().catch(() => {});
  await refresh();
  route();
  setInterval(refresh, REFRESH_MS);
})();
