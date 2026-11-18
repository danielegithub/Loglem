# Comandos

`loglem COMANDO --help` mostra todas as opções. Sem comando, o Loglem abre o menu do terminal. Ponha `--data-dir /caminho/absoluto` antes do comando para usar outra pasta de dados (ou defina `LOGLEM_DATA_DIR`).

## Fontes e leitura

| Comando | O que faz |
|---|---|
| `site add example.com`, `site list`, `site show example.com` | Cria um site, lista os sites ou mostra um site e os seus registos. |
| `site copies example.com`, `site duplicates example.com [--external]` | Conta as localizações dos originais e arquivos e compara as linhas recentes. Com `--external`, pesquisa também ficheiros, journal e contentores locais. |
| `source add-folder / add-file / add-podman / add-docker / add-kubernetes / add-journal` | Adiciona uma fonte. |
| `source list`, `source set NOME --days N` | Lista as fontes, muda a duração. |
| `source configure NOME …` | Nome, pasta dos ficheiros terminados, limpeza dos rodados, caminho. |
| `discover` | Procura os logs na configuração dos programas conhecidos (`src/recipes/`). |
| `add`, `site rename / remove` | Adição guiada (o quê, quem, onde, resumo); mudar o nome ou apagar um site. |
| `podman-list`, `docker-list`, `kubernetes-list` | Lista contentores ou pods. |
| `collect` | Lê uma vez. |
| `serve` / `web` | Lê sempre e serve a página web. |
| `status [--json]` | Locais, tamanhos, datas, estado, falhas. |
| `check [--minutes N]` | Código de saída 1 se o Loglem estiver desligado, uma fonte não for lida ou disparar um alerta. |

## Pesquise e siga

| Comando | O que faz |
|---|---|
| `search` | `--text --ignore-case --level error --ip --path --status --from --to --source --limit --json` |
| `tail` | Mesmos filtros, em direto. `--server URL --user NOME` segue um Loglem remoto. |
| `stats FONTE [--days N] [--json]` | Pedidos por dia, páginas, códigos, IPs, páginas lentas. |

## Cuide dos dados

| Comando | O que faz |
|---|---|
| `alerts list / add-pattern / add-size / add-free-space / remove / command / seen / monitor / email-set / email-test / email-off / email-show` | Regras de alerta e notificação. |
| `backup DIR`, `backup-verify DIR`, `restore DIR [--replace]` | Copia, verifica e restaura os dados. |
| `proof enable / disable / verify` | Modo prova: sela cada dia. |
| `privacy set FONTE …`, `privacy report` | Mascara IPs, mantém campos JSON, lista o que é guardado. |
| `erase … [--originals]`, `export …`, `audit [--days N]` | RGPD: apaga (também dos ficheiros originais) ou exporta as linhas de uma pessoa, vê quem consultou. Também na página **Dados pessoais**. |
| `prune [--apply]` | Mostra ou apaga as linhas expiradas. |
| `archive-completed` | Arquiva e verifica à mão um ficheiro terminado. |

## Execute e mantenha

| Comando | O que faz |
|---|---|
| `service install [--start] / uninstall / start / stop / status` | Serviço: systemd no Linux, launchd no macOS. |
| `access user-add / user-remove / user-list / address / proxy` | Outros computadores: utilizadores, endereço, proxy inverso. |
| `config export / import / poll / web-port` | Definições. |
| `update check / install / status / configure` | Atualizações assinadas (Linux). |
| `diagnostics`, `self-check`, `init` | Erros do Loglem, um teste rápido, uma configuração vazia. |
