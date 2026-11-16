# Referência de comandos

Execute `loglem --help` ou `loglem COMANDO --help` para ver as opções da versão instalada. Sem comando, o Loglem abre o menu de teclado no terminal; `loglem tui` faz o mesmo de forma explícita.

| Comando | Para que serve |
|---|---|
| `init` | Cria uma configuração vazia. |
| `source list` | Lista as fontes. Para adicionar uma: `source add-folder`, `add-file`, `add-podman` ou `add-caddy` (registo do Caddy com limpeza dos ficheiros resultantes da rotação). |
| `podman-list` | Encontra contentores e pods; `--connection` escolhe uma ligação com nome. |
| `status [--json]` | Mostra a localização dos arquivos, tamanhos, datas e estado da recolha. |
| `collect` / `serve` | Recolhe uma vez, ou executa o serviço (recolha mais página web num endereço fixo) até ser parado. `web` faz o mesmo, ou mostra o endereço se o serviço já estiver ativo. |
| `search [...]` | Pesquisa texto e intervalos de datas nos arquivos; `--json` dá uma saída legível por programas. |
| `prune [--apply]` | Mostra as entradas expiradas, ou com `--apply` apaga-as dos arquivos. |
| `config export FILE` / `config import FILE` | Exporta ou importa a configuração das fontes. |
| `config poll N` | Define de quanto em quanto tempo se recolhe automaticamente (em segundos). |
| `config web-port N` | Escolhe a porta local fixa da página web (a partir do próximo arranque). |
| `service install [--start]` | Regista um serviço de utilizador do systemd; `start`, `stop` e `status` permitem geri-lo. |
| `diagnostics` | Mostra os erros do Loglem guardados, com os respetivos rastos da pilha de chamadas. |
| `self-check` | Verifica o executável e o esquema SQLite incorporado sem tocar nos dados. |

Passe `--data-dir /caminho/absoluto` antes do comando para escolher a pasta de dados; `LOGLEM_DATA_DIR` faz o mesmo em automatizações. Os comandos para gerir fontes e atualizações estão descritos em [usage.md](usage.md).
