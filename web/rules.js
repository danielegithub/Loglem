'use strict';

let policyState = { rules: { classes: {} }, alerts: [] };
let serverMap = [];
const classTemplates = {
  'Accessi amministrativi': { period: { amount: 1, unit: 'years' }, seal: true, backup: true },
  'Accessi web': { period: { amount: 90, unit: 'days' }, max_mb: 1024, mask_ips: true },
  Applicazioni: { period: { amount: 14, unit: 'days' }, max_mb: 300 },
  'Cron e sistema': { period: { amount: 30, unit: 'days' } },
};

function options(id, pairs) {
  const select = $(id),
    previous = select.value;
  select.replaceChildren(...pairs.map(([value, label]) => new Option(label, value)));
  if (pairs.some(([v]) => v === previous)) select.value = previous;
}
function tabulate(id, headings, rows) {
  const table = element('table'),
    head = table.createTHead().insertRow(),
    body = table.createTBody();
  for (const text of headings) head.append(element('th', text));
  for (const row of rows) {
    const tr = body.insertRow();
    for (const value of row) {
      const cell = tr.insertCell();
      if (value instanceof Node) cell.append(value);
      else cell.textContent = value;
    }
  }
  $(id).replaceChildren(table);
}
function actionButton(label, action) {
  const button = element('button', label);
  button.type = 'button';
  button.addEventListener('click', () => perform(button, action));
  return button;
}
async function saveRule(values) {
  await api('rule-change', values);
  await refresh();
  await loadRulesPage();
  showNotice('Impostazioni salvate.');
}

async function loadRulesPage() {
  policyState = await api('rules');
  const sources = state.sources.map((s) => [s.source, s.label || s.source]);
  const classes = Object.keys(policyState.rules.classes).sort();
  const config = policyState.rules.remote;
  options('assign-source', sources);
  options('proof-source', sources);
  options('alert-source', [['', 'Tutte le sorgenti'], ...sources]);
  options('assign-class', [['', 'Regola individuale'], ...classes.map((n) => [n, n])]);
  options('class-picker', [
    ['', 'Nuova classe'],
    ...classes.map((n) => [n, n]),
    ...Object.keys(classTemplates)
      .filter((n) => !classes.includes(n))
      .map((n) => [n, 'Modello · ' + n]),
  ]);
  $('known-sites').replaceChildren(
    ...[...new Set(state.sources.map((s) => s.site).filter(Boolean))].map((s) => new Option(s)),
  );
  $('total-quota').value = policyState.rules.max_mb;
  populateClass();
  populateAssignment();
  populateProof();
  const cells = state.sources.map((s) => {
    const rule = policyState.rules.classes[s.class];
    const remote = config && (!config.classes_only || rule?.backup);
    const history = s.earliest
      ? Math.max(0, Math.floor((Date.now() - Date.parse(s.earliest)) / 86400000)) +
        ' giorni dal primo log'
      : 'Nessuna riga';
    return [
      s.label || s.source,
      s.origin + ' · ' + s.archive,
      s.site || '—',
      s.class || 'Individuale',
      rule?.reason || '—',
      periodText(sourcePeriod(s)),
      history,
      formatSize(s.archive_bytes) + (rule?.max_mb ? ' · quota classe ' + rule.max_mb + ' MiB' : ''),
      'SQLite in chiaro',
      s.proof ? (s.proof.tsa ? 'Sigillo + TSA configurata' : 'Sigillo giornaliero') : 'Disattivato',
      remote ? 'OpenPGP · ogni ' + config.hours + ' ore' : 'Non programmato',
      originalRetentionNote(s.input, s),
    ];
  });
  tabulate(
    'rules-table',
    [
      'Sorgente',
      'Origine e archivio',
      'Sito',
      'Classe',
      'Perché',
      'Durata copie',
      'Storico attuale',
      'Spazio',
      'Cifratura archivio',
      'Modalità prova',
      'Backup',
      'Originali',
    ],
    cells,
  );
  $('alert-list').replaceChildren(
    ...policyState.alerts.map((r) => {
      const line = element('p');
      const detail =
        r.kind === 'pattern'
          ? r.text + ' · ' + r.count + ' in ' + r.minutes + ' min'
          : r.megabytes + ' MiB';
      line.append(
        element('span', r.name + ' · ' + detail + ' '),
        actionButton('Rimuovi', () => saveRule({ action: 'remove_alert', name: r.name })),
      );
      return line;
    }),
  );
  $('backup-dest').value = config?.destination || '';
  $('backup-key').value = config?.recipient || '';
  $('backup-hours').value = config?.hours || 24;
  $('backup-selected').checked = !!config?.classes_only;
  $('send-backup').disabled = $('stop-backup').disabled = !config;
  showBackupStatus(policyState.backup_status);
}
function showBackupStatus(s) {
  $('backup-status').textContent =
    (s.message || 'Nessun trasferimento registrato') +
    (s.success ? ' · ultimo verificato ' + new Date(s.success * 1000).toLocaleString('it-IT') : '');
}
function populateAssignment() {
  const s = state.sources.find((s) => s.source === $('assign-source').value);
  $('assign-site').value = s?.site || '';
  $('assign-class').value = s?.class || '';
}
function populateProof() {
  const s = state.sources.find((s) => s.source === $('proof-source').value);
  $('proof-on').checked = !!s?.proof;
  $('proof-tsa').value = s?.proof?.tsa || '';
  $('proof-report').textContent = s?.class ? 'Modalità definita dalla classe ' + s.class : '';
  for (const control of [
    $('proof-on'),
    $('proof-tsa'),
    $('proof-form').querySelector('button:not([type])'),
  ])
    control.disabled = !!s?.class;
}
function populateClass() {
  const name = $('class-picker').value;
  const c = policyState.rules.classes[name] || classTemplates[name] || {};
  $('class-name').value = name;
  $('class-name').readOnly = !!policyState.rules.classes[name];
  $('class-reason').value = c.reason || '';
  $('class-duration').value = c.period?.amount || 28;
  $('class-unit').value = c.period?.unit || 'days';
  $('class-quota').value = c.max_mb || 0;
  $('class-mask').checked = !!c.mask_ips;
  $('class-proof').checked = !!c.seal;
  $('class-tsa').value = c.tsa || '';
  $('class-compress').checked = !!c.compress;
  $('class-backup').checked = !!c.backup;
  $('delete-class').disabled = !policyState.rules.classes[name];
}
function submitForm(id, fn) {
  $(id).addEventListener('submit', (e) => {
    e.preventDefault();
    perform(e.submitter, () => fn(e.submitter));
  });
}
$('class-picker').onchange = populateClass;
$('assign-source').onchange = populateAssignment;
$('proof-source').onchange = populateProof;
submitForm('class-form', async () => {
  const name = $('class-name').value.trim();
  if (
    state.sources.some((s) => s.class === name) &&
    !confirm(
      'Applicare durata, sigilli, mascheramento e quota a tutte le sorgenti della classe? I dati rimossi non sono recuperabili da Loglem.',
    )
  )
    return;
  await saveRule({
    action: 'class',
    name,
    class: {
      reason: $('class-reason').value.trim(),
      period: readPeriod('class-duration', 'class-unit'),
      max_mb: Number($('class-quota').value),
      seal: $('class-proof').checked,
      tsa: $('class-tsa').value || null,
      mask_ips: $('class-mask').checked,
      compress: $('class-compress').checked,
      backup: $('class-backup').checked,
    },
  });
  $('class-picker').value = name;
  populateClass();
});
$('delete-class').onclick = () =>
  perform($('delete-class'), () =>
    saveRule({ action: 'remove_class', name: $('class-picker').value }),
  );
submitForm('assignment', () => {
  const source = $('assign-source').value,
    cls = $('assign-class').value;
  if (
    cls &&
    cls !== state.sources.find((s) => s.source === source)?.class &&
    !confirm(
      'Applicare alla sorgente le regole di conservazione e trattamento della classe ' + cls + '?',
    )
  )
    return;
  return saveRule({ action: 'assign', source, site: $('assign-site').value, class: cls || null });
});
submitForm('quota-form', () => {
  const max_mb = Number($('total-quota').value);
  if (
    max_mb &&
    !confirm(
      'Applicare la quota? Se necessario saranno eliminate le righe più vecchie degli archivi senza sigilli.',
    )
  )
    return;
  return saveRule({ action: 'space', max_mb });
});
submitForm('proof-form', () =>
  saveRule({
    action: 'proof',
    source: $('proof-source').value,
    enabled: $('proof-on').checked,
    tsa: $('proof-tsa').value || null,
  }),
);
$('verify-proof').onclick = () =>
  perform($('verify-proof'), async () => {
    $('proof-report').textContent = (
      await api('proof-verify', { source: $('proof-source').value })
    ).message;
  });
$('alert-kind').onchange = () => {
  const pattern = $('alert-kind').value === 'pattern';
  $('alert-text-label').hidden = $('alert-time-label').hidden = !pattern;
  $('alert-text').required = pattern;
  $('alert-threshold').textContent = pattern ? 'Numero di occorrenze' : 'Soglia in MiB';
  $('alert-source').disabled = $('alert-kind').value === 'free_space';
};
submitForm('alert-form', () => {
  const kind = $('alert-kind').value,
    rule = { name: $('alert-name').value, kind };
  if (kind !== 'free_space') rule.source = $('alert-source').value || null;
  if (kind === 'pattern')
    Object.assign(rule, {
      text: $('alert-text').value,
      count: Number($('alert-value').value),
      minutes: Number($('alert-minutes').value),
    });
  else rule.megabytes = Number($('alert-value').value);
  return saveRule({ action: 'alert', rule });
});
submitForm('local-backup', async (button) => {
  $('local-result').textContent = (
    await api('backup-' + button.value, { target: $('backup-path').value })
  ).message;
});
submitForm('schedule-form', () =>
  saveRule({
    action: 'schedule',
    schedule: {
      hours: Number($('backup-hours').value),
      recipient: $('backup-key').value.trim(),
      destination: $('backup-dest').value.trim(),
      classes_only: $('backup-selected').checked,
    },
  }),
);
$('stop-backup').onclick = () =>
  perform($('stop-backup'), () => saveRule({ action: 'schedule', schedule: null }));
$('send-backup').onclick = () =>
  perform($('send-backup'), async () => {
    $('backup-status').textContent = (await api('backup-transfer')).message;
  });
$('backup-refresh').onclick = () =>
  perform($('backup-refresh'), async () => showBackupStatus((await api('rules')).backup_status));
$('scan-server').onclick = () =>
  perform($('scan-server'), async () => {
    $('map-notes').textContent = 'Scansione in corso…';
    const map = await api('inventory');
    serverMap = map.items;
    $('map-notes').textContent = [serverMap.length + ' sorgenti individuate', ...map.notes].join(
      ' · ',
    );
    renderMap();
  });
$('only-unlinked').onchange = renderMap;
function measureSize(item) {
  if (!['journal', 'podman', 'docker'].includes(item.kind)) return 'Non disponibile';
  const result = element('span');
  result.append(
    actionButton('Misura messaggi', async () => {
      const measured = await api('inventory-size', {
        kind: item.kind,
        journal: item.journal,
        container: item.container,
      });
      result.textContent = formatSize(measured.bytes) + ' di messaggi leggibili';
    }),
  );
  return result;
}
function renderMap() {
  const rows = serverMap
    .filter((i) => !$('only-unlinked').checked || !i.connected.length)
    .map((i) => [
      i.program,
      i.path || i.name,
      i.bytes == null ? measureSize(i) : formatSize(i.bytes),
      i.connected.join(', ') || 'Da collegare',
      i.connected.length
        ? ''
        : actionButton('Collega', () => {
            window.addEventListener(
              'hashchange',
              () => {
                let input = { kind: i.kind };
                if (i.path) input.path = i.path;
                if (i.container) input.container = i.container;
                if (i.journal) input.journal = i.journal;
                addKind = i.kind;
                configureSource(
                  input,
                  i.program + ' · ' + (i.path || i.name),
                  i.name || i.path.split('/').pop(),
                );
                if (['Caddy', 'Nginx', 'Apache', 'Traefik'].includes(i.program))
                  $('source-kind').value = 'access';
              },
              { once: true },
            );
            location.hash = '#add';
          }),
    ]);
  tabulate('map-result', ['Tipo', 'Percorso o nome', 'Spazio originale', 'Collegamenti', ''], rows);
}
$('choose-cron').onclick = () => {
  addKind = 'journal';
  configureSource(
    { kind: 'journal', journal: { identifiers: ['CRON', 'CROND', 'cron', 'crond'] } },
    'Cron · esecuzioni registrate nel journal',
    'cron',
  );
};
$('choose-traefik').onclick = () =>
  perform($('choose-traefik'), async () => {
    addKind = 'traefik';
    prepareFolderStep();
    showWizardStep(2);
    $('folder-step-title').textContent = 'Scegli il file degli accessi di Traefik';
    $('folder-step-help').textContent =
      'Apri la cartella indicata da accessLog.filePath nella configurazione di Traefik.';
    $('folder-preview-help').textContent = 'Scegli il file di accesso. Loglem legge JSON e CLF.';
    await openFolder();
  });
for (const id of ['search-site', 'search-question'])
  $(id).onchange = () => {
    stopLive();
    const scoped = !!$('search-site').value || !!$('search-question').value;
    $('live').disabled = scoped;
    if (scoped) $('live').checked = false;
    search();
  };
