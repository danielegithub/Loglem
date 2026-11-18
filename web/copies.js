'use strict';

let copyScope = '';
let copyRequest = 0;
let copyReport = null;
let copySite = '';
const locationKinds = { original: 'Originale', rotated: 'Vecchio file', completed: 'File chiuso dal programma',
  folder: 'Cartella', archive: 'Archivio Loglem', runtime: 'Container', journal: 'Journal' };

function updateCopyScope(id, visible, logs) {
  $('site-storage').hidden = !visible;
  const scope = JSON.stringify([id, visible, logs.map(s => [s.source, s.input, s.archive,
    s.enabled, s.detached, s.retention, s.completed_dir, s.remove_rotated, s.entries > 0])]);
  if (scope === copyScope) return;
  copyScope = scope;
  copySite = id;
  copyReport = null;
  copyRequest++;
  $('site-matches').open = false;
  $('site-matches').hidden = true;
  if (visible) loadCopyReport(id, false);
}

function currentCopyPage(id, request) {
  return request === copyRequest && location.hash === '#site/' + id;
}

async function loadCopyReport(id, external) {
  const request = ++copyRequest;
  const summary = $('site-space');
  summary.textContent = external ? 'Confronto con i file, il journal e i container locali…' : 'Conteggio in corso…';
  $('site-recount').disabled = $('site-scan').disabled = true;
  try {
    const result = await api(external ? 'site-duplicates' : 'site-copies', { site: id });
    if (!currentCopyPage(id, request)) return;
    copyReport = result;
    const time = new Date().toLocaleTimeString('it-IT');
    summary.textContent = 'Originali misurati · ' + result.original_files + ' file · ' + formatSize(result.original_bytes) +
      (result.incomplete_originals ? ' (misura parziale)' : '') + ' · Archivi Loglem · ' + result.archives + ' · ' +
      formatSize(result.archive_bytes) + ' · Aggiornato alle ' + time;
    renderCopyMatches(result, external);
    detailSignature = '';
    renderSite(id);
  } catch (error) {
    if (currentCopyPage(id, request)) summary.textContent = error.message;
  } finally {
    if (currentCopyPage(id, request)) $('site-recount').disabled = $('site-scan').disabled = false;
  }
}

// Chi ripulisce gli originali di un log, in poche parole (dal conteggio delle posizioni).
function rotationText(source) {
  const group = copyReport?.sources[source.source];
  if (!group) return 'In attesa del conteggio';
  const origin = group.locations.find(l => l.kind !== 'archive');
  return origin ? origin.owner : 'Solo la copia di Loglem';
}

function copyLocations(source) {
  const group = copyReport?.sources[source.source];
  if (!group) return 'In attesa del conteggio';
  const details = element('details');
  details.dataset.key = 'copies-' + source.source;
  details.append(element('summary', 'Dove sono i dati · ' + group.positions + (group.positions === 1 ? ' posizione' : ' posizioni')));
  const list = element('div', undefined, 'copy-list');
  for (const loc of group.locations) {
    const size = loc.bytes != null ? formatSize(loc.bytes) : ({
      absent: 'si crea alla prima lettura', configured: 'dimensione non misurabile',
      unavailable: 'non leggibile'
    }[loc.state] || '');
    const row = element('div', undefined, 'copy-row');
    const head = element('div', undefined, 'copy-head');
    head.append(element('strong', locationKinds[loc.kind]), element('span', size, 'copy-size'));
    // Una riga breve e colorata dice chi cancella il file; la spiegazione sta dietro un clic.
    const owner = element('div', undefined, 'copy-owner copy-' + loc.level);
    owner.append(element('span', loc.owner));
    if (loc.help) {
      const more = element('details', undefined, 'copy-help');
      more.append(element('summary', loc.level === 'warn' ? 'Cosa fare' : 'Dettagli'), element('p', loc.help));
      owner.append(more);
    }
    row.append(head, element('code', loc.path, 'copy-path'), owner);
    list.append(row);
  }
  details.append(list);
  if (group.locations.some(loc => loc.kind === 'rotated')) details.append(element('small', 'Questi file contengono eventi precedenti a quelli del file in uso.'));
  // Per i file la spiegazione sta già nelle righe sopra; per container e journal serve la durata.
  if (source.input.kind !== 'file' && source.input.kind !== 'folder') {
    const retention = element('details');
    retention.dataset.key = 'retention-' + source.source;
    retention.append(element('summary', 'Per quanto tempo restano gli originali'));
    retention.addEventListener('toggle', () => {
      if (retention.open && retention.childElementCount === 1) retention.append(originalRetentionNote(source.input, source));
    });
    details.append(retention);
  }
  return details;
}

function renderCopyMatches(report, external) {
  const list = $('site-match-list');
  const details = $('site-matches');
  list.replaceChildren();
  details.hidden = false;
  const count = report.matches.length;
  const summary = count ? count + (count === 1 ? ' possibile doppione' : ' possibili doppioni') :
    'Nessuna coincidenza nei campioni confrontati';
  $('site-match-count').textContent = summary +
    (report.external ? ' · ' + formatSize(report.external.identical_bytes) + ' di contenuto identico verificato' : '');
  if (external) details.open = true;

  const sources = new Map(state.sources.map(s => [s.source, s]));
  for (const match of report.matches) {
    const source = sources.get(match.source);
    const other = match.external ? null : sources.get(match.other);
    const item = element('div', undefined, 'copy-match');
    item.append(element('strong', (source?.label || match.source) + ' · Doppione di ' + (other?.label || match.other)),
      element('small', match.common + ' righe uguali su ' + match.compared + ' confrontate'));
    if (other && source && [other, source].every(s => s.enabled && !s.detached) && !document.body.classList.contains('reader')) {
      item.append(element('small', 'Per raccogliere una volta sola, metti in pausa una delle due sorgenti. Le righe già raccolte restano fino alla loro scadenza.'));
      for (const s of [source, other]) {
        const button = actionButton('Metti in pausa ' + (s.label || s.source), async () => {
          await api('duplicate-pause', { site: copySite, name: s.source,
            keep: s === source ? other.source : source.source });
          await refresh(true);
          showNotice('Raccolta in pausa per ' + (s.label || s.source) + '.');
        });
        item.append(button);
      }
    } else if (match.external) {
      item.append(element('small', 'Le stesse righe arrivano due volte. Tieni una sola uscita: nel programma scegli se scrivere su file o nel journal, poi metti in pausa la sorgente che non serve.'));
    }
    list.append(item);
  }
  if (report.unsampled.length) list.append(element('p', 'Senza righe confrontabili · ' +
    report.unsampled.map(id => sources.get(id)?.label || id).join(', ')));
  if (report.external) {
    const scan = report.external;
    for (const group of scan.identical) {
      const item = element('div', undefined, 'copy-match');
      item.append(element('strong', group.paths.length + ' file identici · ' + formatSize(group.bytes_each) + ' ciascuno'));
      for (const path of group.paths) item.append(element('small', path, 'path'));
      item.append(element('small', 'Sono copie uguali e Loglem non le cancella. Scopri quale programma le scrive e togli tu la copia che non ti serve.'));
      list.append(item);
    }
    list.append(element('p', 'Controllati ' + scan.files + ' file esterni e ' + scan.streams + ' flussi locali.' +
      (scan.limited ? ' Raggiunto un limite della ricerca.' : '') +
      (scan.unavailable.length ? ' Non verificati · ' + scan.unavailable.join(', ') + '.' : '')));
  }
  list.append(element('small', 'Confrontiamo fino a 200 righe recenti per log. Se coincide più della metà del campione più grande (per esempio 120 righe su 200), segnaliamo un possibile doppione.'));
}

document.addEventListener('DOMContentLoaded', () => {
  $('site-scan').addEventListener('click', () => loadCopyReport(copySite, true));
  $('site-recount').addEventListener('click', () => loadCopyReport(copySite, false));
});
