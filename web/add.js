'use strict';

// Il percorso guidato «Aggiungi»: 1 cosa · 2 chi · 3 dove · 4 riepilogo.
// Le proposte arrivano dal server (src/wizard.rs), che usa le ricette: qui non c'è nessun
// nome di programma.

const KIND_OPTIONS = [
  ['access', 'Accessi'], ['error', 'Errori'], ['application', 'Applicazione'],
  ['database', 'Database'], ['admin_access', 'Accessi di amministratori'],
  ['system', 'Sistema'], ['other', 'Altro'],
];

let addCatalog = null;
let add = null;
let pickFolder = null;
let pickChoice = null;
let pendingManual = null;

// Dalla Mappa del server: si apre «Aggiungi» con quel log già nell'elenco.
function addFromMap(source, title) {
  pendingManual = { source, title };
  if (location.hash === '#add') return openAdd(null);
  location.hash = '#add';
  return Promise.resolve();
}

const ADD_PANELS = ['add-what', 'add-who', 'add-where', 'folder-step', 'container-step', 'journal-step', 'other-step', 'add-summary'];

function showAddPanel(id, step) {
  for (const panel of ADD_PANELS) $(panel).hidden = panel !== id;
  for (const item of document.querySelectorAll('#view-add [data-step]')) {
    item.classList.toggle('current', Number(item.dataset.step) === step);
  }
  hideNotice();
}

async function openAdd(siteId) {
  add = {
    site: siteId || null,
    category: null,
    name: siteId && state.sites[siteId] ? state.sites[siteId].name : '',
    programs: [],
    proposals: [],
    selected: new Set(),
    warnings: [],
  };
  $('add-name-form').hidden = true;
  showAddPanel('add-what', 1);
  if (!addCatalog) addCatalog = await api('wizard-categories');
  const grid = $('category-grid');
  grid.replaceChildren(...addCatalog.categories.map((c) => {
    const card = element('button', undefined, 'type-card');
    card.type = 'button';
    card.append(element('strong', c.title), element('span', c.description), element('span', '›', 'type-link'));
    card.addEventListener('click', () => chooseCategory(c));
    return card;
  }));
  if (pendingManual) {
    const { source, title } = pendingManual;
    pendingManual = null;
    chooseCategory(addCatalog.categories.find((c) => c.id === 'other'));
    await addManual(source, title);
  }
}

function chooseCategory(category) {
  add.category = category;
  add.programs = [];
  if (category.id === 'other') {
    add.proposals = [];
    add.selected = new Set();
    add.warnings = [];
    renderProposals();
    showAddPanel('add-where', 3);
    return;
  }
  if (category.names_site && !add.site) {
    $('add-name-label').textContent = 'Come si chiama?';
    $('add-name').placeholder = category.name_hint || '';
    $('add-name').value = add.name;
    $('add-name-help').textContent = 'Diventa un sito: ci trovi tutti i suoi log. Loglem cerca anche i log con questo nome.';
    $('add-name-form').hidden = false;
    $('add-name').focus();
    return;
  }
  afterName();
}

function afterName() {
  if (add.category.asks_program) perform($('add-who-next'), loadPrograms);
  else perform($('add-where-next'), loadProposals);
}

// ---------- passo 2 ----------

async function loadPrograms() {
  showAddPanel('add-who', 2);
  $('add-who-title').textContent = add.category.recipes === 'web' || ['web', 'service'].includes(add.category.id)
    ? 'Chi risponde alle richieste?'
    : 'Quale programma lo fa funzionare?';
  const list = $('program-list');
  list.replaceChildren(element('p', 'Cerco sul server i programmi presenti…', 'muted'));
  $('add-who-question').hidden = true;
  const data = await api('wizard-programs', { category: add.category.id, name: add.name });
  list.replaceChildren(...data.programs.map((p) => {
    const label = element('label', undefined, 'container-choice');
    const box = element('input');
    box.type = 'checkbox';
    box.value = p.id;
    box.checked = add.programs.includes(p.id);
    const text = element('div');
    text.append(element('strong', p.name), element('small', p.found.length
      ? 'Trovato: ' + p.found.join(' · ') + (p.has_site ? ' · il sito è nella sua configurazione' : '')
      : 'Non trovato su questo server'));
    label.append(box, text);
    return label;
  }));
  if (data.suggested && data.question) {
    $('add-who-question-text').textContent = data.question;
    $('add-who-question').hidden = false;
    $('add-who-yes').onclick = () => {
      add.programs = [data.suggested];
      perform($('add-who-yes'), loadProposals);
    };
  }
}

function chosenPrograms() {
  return [...document.querySelectorAll('#program-list input:checked')].map((b) => b.value);
}

// ---------- passo 3 ----------

async function loadProposals() {
  showAddPanel('add-where', 3);
  $('proposal-list').replaceChildren(element('p', 'Cerco i log: file, container e servizi del journal…', 'muted'));
  $('add-where-warnings').replaceChildren();
  const data = await api('wizard-proposals', { category: add.category.id, name: add.name, programs: add.programs });
  add.proposals = data.proposals;
  add.warnings = data.warnings;
  add.selected = new Set(data.proposals.filter((p) => p.exact && p.readable).map((p) => p.key));
  renderProposals();
}

function proposalRow(p) {
  const row = element('div', undefined, 'proposal' + (p.readable ? '' : ' unreadable'));
  const label = element('label', undefined, 'proposal-head');
  const box = element('input');
  box.type = 'checkbox';
  box.checked = add.selected.has(p.key);
  box.disabled = !p.readable;
  box.addEventListener('change', () => {
    if (box.checked) add.selected.add(p.key);
    else add.selected.delete(p.key);
  });
  const text = element('div');
  text.append(element('strong', p.title, 'proposal-title'));
  const meta = [p.reason, p.program_name, p.format, p.bytes != null ? formatSize(p.bytes) : null]
    .filter(Boolean).join(' · ');
  text.append(element('small', meta));
  label.append(box, text);
  row.append(label);
  if (p.problem) row.append(element('p', p.problem, 'card-warning'));
  if (p.preview.length) {
    const preview = element('details', undefined, 'proposal-preview');
    preview.append(element('summary', 'Ultime righe'), element('pre', p.preview.join('\n'), 'snippet'));
    row.append(preview);
  }
  return row;
}

function renderProposals() {
  const warnings = $('add-where-warnings');
  warnings.replaceChildren(...add.warnings.map((w) => element('p', w, 'card-warning')));
  const list = $('proposal-list');
  if (add.proposals.length === 0) {
    list.replaceChildren(element('p', add.category.id === 'other'
      ? 'Scegli qui sotto dove sta il log.'
      : 'Non ho trovato niente da solo: scegli qui sotto dove sta il log.', 'muted'));
    return;
  }
  list.replaceChildren(...add.proposals.map(proposalRow));
}

// Un log scelto a mano entra nell'elenco già spuntato.
async function addManual(source, title) {
  const p = await api('wizard-manual', { source, kind: add.category.kind, title });
  add.proposals = add.proposals.filter((o) => o.key !== p.key).concat([p]);
  add.selected.add(p.key);
  renderProposals();
  showAddPanel('add-where', 3);
}

// ---------- «Aggiungi da…»: file o cartella ----------

async function openPickFolder(path = '') {
  pickFolder = await api('folders', { path });
  $('folder-path').value = pickFolder.path;
  $('folder-parent').disabled = !pickFolder.parent;
  const items = $('folder-items');
  items.replaceChildren(...pickFolder.folders.map((sub) => {
    const button = element('button', '▱  ' + sub.split('/').pop());
    button.type = 'button';
    button.addEventListener('click', () => perform(button, () => openPickFolder(sub)));
    return button;
  }));
  if (pickFolder.folders.length === 0) items.append(element('p', 'Nessuna sottocartella.', 'muted'));
  const files = pickFolder.files || [];
  $('log-count').textContent = files.length === 0 ? 'Nessun file in questa cartella' : 'Scegli un file:';
  $('log-files').replaceChildren(...files.map((f) => {
    const item = element('li');
    const name = f.path.split('/').pop();
    const button = element('button', name + (f.log ? '' : ' (file di testo?)') + ' — usa questo file →');
    button.type = 'button';
    button.addEventListener('click', () => perform(button, () => addManual({ kind: 'file', path: f.path }, f.path)));
    item.append(button);
    return item;
  }));
  try {
    const { patterns } = await api('folder-patterns', { path: pickFolder.path });
    $('folder-pattern-options').replaceChildren(...patterns.map((p) => new Option(p.pattern + ' · ' + p.files + ' file', p.pattern)));
    $('folder-pattern').value = patterns.length ? patterns[0].pattern : '*.log';
  } catch (_) {
    $('folder-pattern').value = '*.log';
  }
}

// ---------- «Aggiungi da…»: container e pod ----------

async function openPickContainer() {
  pickChoice = null;
  $('use-container').disabled = true;
  $('follow-name-box').hidden = true;
  $('follow-app-box').hidden = true;
  const list = $('container-list');
  list.replaceChildren(element('p', 'Cerco container e pod…', 'muted'));
  const { containers, pods } = await api('containers');
  list.replaceChildren();
  if (!containers.length && !pods.length) {
    list.append(element('p', 'Non vedo container né pod: Podman, Docker e kubectl non rispondono all\'utente che esegue Loglem.', 'muted'));
    return;
  }
  const choice = (title, details, onPick) => {
    const label = element('label', undefined, 'container-choice');
    const radio = element('input');
    radio.type = 'radio';
    radio.name = 'add-container';
    radio.addEventListener('change', onPick);
    const text = element('div');
    text.append(element('strong', title), element('small', details));
    label.append(radio, text);
    list.append(label);
  };
  for (const c of containers) {
    const runtime = c.runtime === 'docker' ? 'Docker' : 'Podman';
    choice(c.name, runtime + ' · ' + (c.image || 'immagine sconosciuta') + ' · ' + (c.state || ''), async () => {
      pickChoice = { container: c };
      $('use-container').disabled = false;
      $('follow-app-box').hidden = true;
      $('follow-name-box').hidden = true;
      try {
        const logs = await api('container-logs', { runtime: c.runtime, connection: null, container: c.id });
        if (pickChoice?.container?.id !== c.id) return;
        $('follow-name-box').hidden = !logs.journald;
        $('follow-by-name').checked = true;
      } catch (_) {
        // Facoltativo: senza questa informazione resta la lettura normale del container.
      }
    });
  }
  for (const pod of pods) {
    choice('pod ' + pod.name, 'Kubernetes · ' + pod.namespace + (pod.app ? ' · app=' + pod.app : '') + ' · ' + pod.phase, () => {
      pickChoice = { pod };
      $('use-container').disabled = false;
      $('follow-name-box').hidden = true;
      $('follow-app-box').hidden = !pod.app;
    });
  }
}

function usePickedContainer() {
  if (!pickChoice) return Promise.resolve();
  if (pickChoice.container) {
    const c = pickChoice.container;
    if (!$('follow-name-box').hidden && $('follow-by-name').checked) {
      const journal = { units: [], identifiers: [], matches: ['CONTAINER_NAME=' + c.name], priority: null };
      return addManual({ kind: 'journal', journal, journal_runtime: c.runtime }, 'container «' + c.name + '» (per nome, dal journal)');
    }
    return addManual({ kind: c.runtime, container: c.id }, 'container «' + c.name + '» · ' + (c.runtime === 'docker' ? 'Docker' : 'Podman'));
  }
  const pod = pickChoice.pod;
  const byApp = pod.app && $('kubernetes-follow-app').checked;
  const kubernetes = byApp
    ? { namespace: pod.namespace, selector: 'app=' + pod.app }
    : { namespace: pod.namespace, pod: pod.name };
  return addManual({ kind: 'kubernetes', kubernetes }, byApp
    ? 'pod dell\'applicazione «' + pod.app + '» · ' + pod.namespace
    : 'pod ' + pod.name + ' · ' + pod.namespace);
}

// ---------- «Aggiungi da…»: journal ----------

async function openPickJournal() {
  const list = $('journal-units');
  list.replaceChildren(element('p', 'Leggo i servizi presenti nel journal…', 'muted'));
  try {
    const { units } = await api('journal-units');
    list.replaceChildren(...units.map((unit) => {
      const label = element('label', undefined, 'container-choice');
      const box = element('input');
      box.type = 'checkbox';
      box.value = unit;
      label.append(box, element('strong', unit));
      return label;
    }));
    if (!units.length) list.append(element('p', 'Nessun servizio trovato.', 'muted'));
  } catch (error) {
    list.replaceChildren(element('div', error.message, 'card-error'));
  }
}

function pickedJournal() {
  const units = [...document.querySelectorAll('#journal-units input:checked')].map((b) => b.value);
  const identifiers = $('journal-identifiers').value.split(',').map((s) => s.trim()).filter(Boolean);
  const value = $('journal-priority').value;
  const priority = value === '' ? null : Number(value);
  if (!units.length && !identifiers.length && priority === null) {
    throw new Error('Scegli almeno un servizio, un programma o una gravità: il journal intero è troppo.');
  }
  const parts = [];
  if (units.length) parts.push(units.join(', '));
  if (identifiers.length) parts.push(identifiers.join(', '));
  if (priority !== null) parts.push('gravità fino a ' + priority);
  return { filter: { units, identifiers, matches: [], priority }, title: 'journal · ' + parts.join(' · ') };
}

// ---------- passo 4 ----------

function chosenProposals() {
  return add.proposals.filter((p) => add.selected.has(p.key));
}

function renderSummary() {
  const target = add.site
    ? 'Troverai questi log in «' + state.sites[add.site].name + '».'
    : add.category.names_site
      ? 'I log vanno nel sito «' + add.name + '» (nuovo, se non esiste già).'
      : 'I log vanno in «Questo server».';
  $('add-summary-target').textContent = target;
  const rows = chosenProposals().map((p) => {
    const select = element('select');
    for (const [value, text] of KIND_OPTIONS) select.append(new Option(text, value));
    select.value = p.kind;
    select.addEventListener('change', () => { p.kind = select.value; });
    const where = element('div', p.title);
    where.append(element('small', [p.program_name, p.format].filter(Boolean).join(' · ') || 'Programma riconosciuto dal contenuto, dopo la prima lettura'));
    return [select, where, '28 giorni'];
  });
  tabulate('add-summary-list', ['Tipo', 'Da dove', 'Durata in Loglem'], rows);
}

async function saveAdd() {
  const items = chosenProposals().map((p) => ({ source: p.source, kind: p.kind }));
  const result = await api('wizard-save', {
    category: add.category.id,
    name: add.name,
    site: add.site,
    items,
  });
  await refresh(true);
  location.hash = '#site/' + (result.site || '~');
  if (result.sources.length) watchNewSource(result.sources[0], result.site);
  showNotice(result.sources.length + (result.sources.length === 1 ? ' log collegato' : ' log collegati') +
    '. Sto leggendo: le righe compariranno qui da sole.');
}

// ---------- pulsanti ----------

$('add-name-form').addEventListener('submit', (event) => {
  event.preventDefault();
  const name = $('add-name').value.trim();
  if (!name) {
    showNotice('Indica un nome.', true);
    return;
  }
  add.name = name;
  afterName();
});
$('add-name-cancel').addEventListener('click', () => { $('add-name-form').hidden = true; });

$('add-who-no').addEventListener('click', () => { $('add-who-question').hidden = true; });
$('add-who-next').addEventListener('click', () => {
  add.programs = chosenPrograms();
  perform($('add-who-next'), loadProposals);
});
for (const id of ['add-who-unknown', 'add-who-other']) {
  // «Non lo so» e «Un altro programma» non bloccano: i log si scelgono al passo dopo.
  $(id).addEventListener('click', () => {
    add.programs = [];
    perform($(id), loadProposals);
  });
}

$('add-where-next').addEventListener('click', () => {
  if (!chosenProposals().length) {
    showNotice('Scegli almeno un log, oppure aggiungilo con «Manca qualcosa?».', true);
    return;
  }
  renderSummary();
  showAddPanel('add-summary', 4);
});

$('from-files').addEventListener('click', () => perform($('from-files'), async () => {
  showAddPanel('folder-step', 3);
  await openPickFolder(pickFolder?.path || '');
}));
$('from-container').addEventListener('click', () => perform($('from-container'), async () => {
  showAddPanel('container-step', 3);
  await openPickContainer();
}));
$('from-journal').addEventListener('click', () => perform($('from-journal'), async () => {
  showAddPanel('journal-step', 3);
  await openPickJournal();
}));
$('from-other').addEventListener('click', () => {
  $('other-help').textContent = addCatalog?.other_help || '';
  showAddPanel('other-step', 3);
});

$('open-folder').addEventListener('click', () => perform($('open-folder'), () => openPickFolder($('folder-path').value)));
$('folder-path').addEventListener('keydown', (event) => {
  if (event.key === 'Enter') {
    event.preventDefault();
    $('open-folder').click();
  }
});
$('folder-parent').addEventListener('click', () => perform($('folder-parent'), () => openPickFolder(pickFolder.parent)));
$('use-folder').addEventListener('click', () => perform($('use-folder'), () => {
  const pattern = $('folder-pattern').value.trim() || '*.log';
  return addManual({ kind: 'folder', path: pickFolder.path, pattern }, pickFolder.path + ' (' + pattern + ')');
}));
$('use-container').addEventListener('click', () => perform($('use-container'), usePickedContainer));
$('use-journal').addEventListener('click', () => perform($('use-journal'), () => {
  const { filter, title } = pickedJournal();
  return addManual({ kind: 'journal', journal: filter }, title);
}));

for (const button of document.querySelectorAll('[data-add-back]')) {
  button.addEventListener('click', () => {
    const step = Number(button.dataset.addBack);
    if (step === 1) showAddPanel('add-what', 1);
    else if (step === 2 && add.category?.asks_program) showAddPanel('add-who', 2);
    else if (step === 2) showAddPanel('add-what', 1);
    else showAddPanel('add-where', 3);
  });
}

$('add-save').addEventListener('click', () => perform($('add-save'), saveAdd));
