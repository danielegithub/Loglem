# Usar o Loglem

Um guia passo a passo. Para a lista de comandos, veja [cli.md](cli.md); para o que o Loglem não faz, veja [limitations.md](limitations.md).

## Índice

- [1. Adicione a sua primeira fonte](#1-adicione-a-sua-primeira-fonte)
- [2. Pesquise nos registos](#2-pesquise-nos-registos)
- [3. Escolha durante quanto tempo guardar os registos](#3-escolha-durante-quanto-tempo-guardar-os-registos)
- [4. Mantenha o Loglem sempre ligado](#4-mantenha-o-loglem-sempre-ligado)
- [5. Deixe o Loglem limpar os ficheiros originais](#5-deixe-o-loglem-limpar-os-ficheiros-originais)
- [6. Altere, pause ou retire uma fonte](#6-altere-pause-ou-retire-uma-fonte)
- [7. Onde estão os seus dados e como os mover](#7-onde-estão-os-seus-dados-e-como-os-mover)
- [8. Quando algo corre mal](#8-quando-algo-corre-mal)
- [9. Atualize o Loglem](#9-atualize-o-loglem)

## 1. Adicione a sua primeira fonte

Uma **fonte** é o sítio de onde vêm os registos: uma pasta de ficheiros `.log`, um ficheiro avulso, um contentor Podman ou o registo de acesso do Caddy. Pode adicioná-la pela página web, pelo menu do terminal ou pela linha de comandos.

### Pela página web

```sh
loglem web
```

O Loglem mostra o endereço da página (por exemplo `http://127.0.0.1:53142`) e a pasta onde guarda os dados. Abra esse endereço num navegador. O endereço mantém-se igual depois de reiniciar. Se a porta estiver ocupada por outro programa, o Loglem para e di-lo; escolha outra com `loglem config web-port 8790`. O Loglem nunca usa a porta 80 nem a 443.

A página só funciona na máquina onde o Loglem está a correr. Num servidor remoto, abra-a com um túnel SSH (`ssh -L 53142:127.0.0.1:53142 utilizador@servidor` e depois vá a `http://127.0.0.1:53142`).

O menu da esquerda tem quatro páginas:

- **Sorgenti:** as suas fontes, com estado, número de linhas e ligações para os registos e as definições.
- **Esplora i log:** pesquise e leia os registos de uma fonte. A lista atualiza-se sozinha; uma caixa de seleção permite pausar a atualização.
- **Aggiungi sorgente:** um formulário guiado. Escolha o tipo, a pasta ou o contentor, um nome, que tipo de registo é e durante quanto tempo guardá-lo, e prima **Salva e apri i log**. O Loglem lê os registos de imediato.
- **Problemi:** os erros do próprio Loglem, com a respetiva causa (veja a [secção 8](#8-quando-algo-corre-mal)).

### Pela linha de comandos

```sh
# uma pasta de ficheiros .log (os ficheiros novos são detetados automaticamente)
loglem source add-folder --name web-logs --path /srv/example/logs --days 28

# um ficheiro avulso; lê também as cópias renomeadas, como api.log-1
loglem source add-file --name api --path /srv/example/logs/api.log \
  --rotated-pattern 'api.log-*' --days 14

# um contentor Podman
loglem podman-list
loglem source add-podman --name app --container my-container --days 14

# ler agora e verificar
loglem collect
loglem status
```

Notas:

- Uma fonte de pasta lê os ficheiros `.log` que estão diretamente na pasta; subpastas, ficheiros comprimidos e outras extensões são ignorados.
- Um ficheiro tem de ter um nome que termine em `.log`. Os ficheiros comprimidos (`.gz`) não são lidos.
- Com o Podman, o Loglem memoriza o ID completo do contentor. Se recriar um contentor com o mesmo nome, adicione-o como fonte nova (ou veja a [secção 6](#6-altere-pause-ou-retire-uma-fonte)). O histórico já recolhido continua disponível depois de o contentor ser removido. Com `--connection NOME` escolhe uma ligação Podman com nome.
- O Loglem só lê os seus ficheiros originais. Não os altera, a não ser que ative a limpeza (secção 5).
- Os nomes e botões da página e do menu estão, por agora, em italiano.

### Pelo menu do terminal

Execute `loglem`. Use as setas (ou `j`/`k`), Enter para escolher e Esc para voltar atrás.

```text
> Le mie sorgenti — posizione, spazio e log salvati
  Aggiungi cartella, file o container
  Cerca nei log
  Conservazione — per quanti giorni tenere i log
  Leggi i log adesso
  Raccolta automatica (servizio Linux)
  Impostazioni
  Aggiornamenti
  Esci
```

Escolha **Aggiungi cartella, file o container**, depois o tipo e navegue até à pasta (ou escolha **Scrivi / incolla un percorso…** para introduzir o caminho). Confira o resumo e guarde com **Salva e leggi i log adesso**. Esc cancela antes de guardar.

## 2. Pesquise nos registos

Na página web abra **Esplora i log**, escolha a fonte e escreva o texto. Pode limitar por data (UTC). As linhas mais recentes aparecem primeiro. Pela linha de comandos:

```sh
loglem search --source web-logs --text '/login' --from 2026-11-01 --to 2026-11-16
```

- A pesquisa é de texto exato e distingue maiúsculas de minúsculas; não é uma expressão regular.
- `--to 2026-11-16` inclui todo esse dia (UTC).
- Acrescente `--json` para uma saída legível por outros programas e `--limit` para obter até 1000 resultados.
- Se uma linha não tem uma data que o Loglem reconheça, continua pesquisável e é mostrada com a hora a que foi lida. O Loglem reconhece os registos JSON mais comuns (`ts`, `timestamp`, `time`, `@timestamp`).

## 3. Escolha durante quanto tempo guardar os registos

Cada fonte tem o seu próprio período de conservação: um número de dias, meses ou anos. Quando uma entrada fica mais antiga do que isso, o Loglem apaga-a do seu arquivo.

```sh
loglem source set web-logs --days 7
loglem source configure web-logs --amount 6 --unit months --log-kind access
loglem source configure web-logs --amount 2 --unit years
```

- Na página web e no menu do terminal (**Conservazione**), a mesma definição é um formulário que mostra também quando as entradas vão expirar.
- As entradas expiradas são removidas enquanto o Loglem está a correr (veja a secção 4). `loglem prune` mostra o que seria apagado; `loglem prune --apply` apaga-o já.
- A conservação conta a partir da data da linha ou de quando o Loglem a leu pela primeira vez, a mais antiga das duas: uma linha com data no futuro não fica para sempre.
- Pausar uma fonte para as novas leituras, não a conservação.
- O tipo de registo (acesso, geral, erros) apenas descreve a fonte; não define por si um prazo legal.
- A duração aplica-se à cópia do Loglem. Os seus ficheiros originais não são apagados, a não ser que ative a limpeza (secção 5).

## 4. Mantenha o Loglem sempre ligado

O `loglem web` só recolhe registos enquanto está a correr. Para recolher sempre, mesmo depois de fechar o terminal ou reiniciar, instale-o como serviço:

```sh
loglem service install --start
loglem service status
```

Isto cria um **serviço de utilizador do systemd** para o utilizador atual. Lê as linhas novas de 10 em 10 segundos (altere com `loglem config poll 5`), remove as entradas expiradas e serve a página web no mesmo endereço.

- Num servidor, peça a um administrador que execute uma vez `sudo loginctl enable-linger UTILIZADOR`, para o serviço correr mesmo quando o utilizador não tem sessão iniciada.
- O serviço só tem as permissões desse utilizador. Verifique se esse utilizador consegue ler as pastas de registos; o Loglem não corre como root.
- Só corre um Loglem por pasta de dados. Se o serviço já estiver ativo, o `loglem web` não inicia um segundo: mostra o endereço do serviço e termina.
- Iniciar o Loglem com outro `--data-dir` mostra outras fontes. A pasta de dados em uso é sempre indicada na página web e no menu do terminal.

## 5. Deixe o Loglem limpar os ficheiros originais

Por predefinição, o Loglem não apaga os seus ficheiros originais. Pode deixá-lo apagar ficheiros originais **depois de os copiar e verificar**. Só o faz com ficheiros que ninguém modificou há pelo menos 24 horas, nunca com o ficheiro em que um programa está a escrever, e guarda qualquer ficheiro que não consiga verificar (a ficha da fonte mostra um aviso). A expiração das entradas arquivadas é um passo à parte, definido na secção 3.

### Caddy

Em **Aggiungi sorgente** escolha **Il log di accesso di Caddy**, abra a pasta dos registos (muitas vezes `/var/log/caddy`) e escolha o ficheiro onde o Caddy escreve, normalmente `access.log`. O Loglem passa a ler também os ficheiros que o Caddy cria quando efetua a rotação do registo (por exemplo `access-2026-11-16T10-00-00.000.log`) e apaga-os depois de verificar. No fim, a página mostra o bloco a colocar no seu Caddyfile:

```caddyfile
log {
	output file /var/log/caddy/access.log {
		roll_size 50MiB
		roll_uncompressed
		roll_keep 100
		roll_keep_for 60d
	}
}
```

- `roll_uncompressed` é obrigatório: o Loglem só verifica ficheiros não comprimidos. Se encontrar ficheiros `.gz`, assinala-os na ficha.
- `roll_keep_for 60d` é uma rede de segurança: se o Loglem ficar desligado muito tempo, o Caddy apaga na mesma os ficheiros mais antigos.
- O Loglem não altera o seu Caddyfile; deve colar o bloco manualmente.

Pela linha de comandos: `loglem source add-caddy --name site --path /var/log/caddy/access.log --days 90`. Para uma fonte de ficheiro que já tem um padrão de rotação: `loglem source configure site --remove-rotated true`.

### Outros programas

Peça ao programa que mova cada ficheiro terminado, fechado e não comprimido para uma pasta dedicada. Nas definições da fonte ative **Archivia ed elimina i file di una cartella dedicata** e confirme. Nunca selecione uma pasta onde um programa ainda esteja a escrever.

```sh
loglem source configure web --completed-dir /absolute/completed-files --handoff-confirmed
# para o desativar de novo
loglem source configure web --completed-dir ""
```

Para um único ficheiro terminado, sem esperar 24 horas:

```sh
loglem archive-completed --source web --file /srv/example/logs/access-old.log --producer-closed --remove
```

Sem `--remove`, o comando arquiva e verifica o ficheiro sem apagar o original. O Loglem nunca apaga os originais de um contentor Podman.

## 6. Altere, pause ou retire uma fonte

Na página web, prima **Impostazioni** na ficha da fonte. Pela linha de comandos:

```sh
loglem source rename web "Site público"
loglem source pause web          # para a leitura, o histórico continua pesquisável
loglem source resume web
loglem source detach web         # desliga; o arquivo continua pesquisável
loglem source configure web --container NEW_CONTAINER   # aponta para um contentor substituto
loglem source configure web --path /new/path            # novo ficheiro ou pasta
```

- Mudar o nome altera só a etiqueta; o histórico mantém-se.
- Para guardar o arquivo de uma fonte noutra pasta: `loglem source configure web --archive-dir /pasta/absoluta`. O Loglem copia-o, verifica a cópia e depois remove os ficheiros antigos; nunca substitui uma base de dados existente. Com `--archive-dir ""` volta à pasta predefinida. Feche antes os outros programas que tenham o arquivo aberto.

## 7. Onde estão os seus dados e como os mover

Por predefinição, tudo fica na pasta do Loglem:

```text
Loglem/
├── loglem          o programa
└── data/
    ├── config.json   as suas fontes e definições
    ├── archives/     os registos guardados (uma base de dados por fonte)
    ├── diagnostics/  o registo de erros do Loglem
    └── updates/
```

Para usar outra localização, passe `--data-dir /caminho/absoluto` antes do comando, ou defina `LOGLEM_DATA_DIR`.

Para passar para outra máquina:

```sh
loglem config export ./loglem-config.json     # na máquina antiga
loglem config import ./loglem-config.json     # na máquina nova
```

Isto move as suas **definições** (fontes, durações), não os registos. Ajuste caminhos e permissões na máquina nova. Para mover também os registos, pare o Loglem e copie toda a pasta `data/`. Importar uma configuração nunca retira a regra de conservação de uma fonte que já tem arquivo; nesse caso, pause essa fonte.

## 8. Quando algo corre mal

- **Uma fonte mostra um erro:** as outras continuam a funcionar e as linhas já guardadas continuam legíveis. Abra a ficha da fonte para ver a mensagem, ou execute `loglem status`.
- **A página está vazia ou uma fonte diz «não é possível ler»:** verifique se o utilizador que executa o Loglem consegue ler a pasta ou o ficheiro.
- **Ver os erros do Loglem:** **Problemi** na página web, ou `loglem diagnostics`. Cada entrada tem a hora, o que o Loglem estava a fazer, a causa e os detalhes técnicos. O ficheiro é `data/diagnostics/loglem.log` (cerca de 5 MB, depois roda; só o proprietário o pode ler).
- **Verificar a instalação:** `loglem self-check`.
- **Porta já em uso:** `loglem config web-port 8790` e reinicie.

## 9. Atualize o Loglem

O mais simples é deixar o Loglem atualizar-se sozinho a partir de uma versão assinada. A pasta `data/` mantém-se. (O `install.sh` não substitui uma instalação existente; para substituir o programa à mão, pare primeiro o serviço.)

```sh
loglem update check
loglem update install
loglem update status
```

Só funciona para um Loglem instalado numa pasta onde o utilizador pode escrever e registado com `loglem service install`. Antes de substituir seja o que for, verifica a assinatura, a versão e o tamanho; se a versão nova não arrancar, o programa anterior fica guardado ao lado (`loglem.previous`). Para um programa compilado localmente, indique primeiro onde procurar e em que chave confiar: `loglem update configure --repository PROPRIETARIO/REPOSITORIO --public-key CHAVE_BASE64`. As atualizações que alteram o formato dos dados são recusadas.
