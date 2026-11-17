# Dados pessoais e modo prova

Os logs contêm dados pessoais. Um endereço IP é um dado pessoal. O Loglem dá-lhe ferramentas para cumprir as regras. **Não** torna uma instalação conforme por si só: porque guarda logs, por quanto tempo e o que diz às pessoas continuam a ser decisões suas.

## Guarde menos

- **Conservação curta.** Defina os dias por fonte. As linhas expiradas são apagadas.
- **Mascare os IPs.** `203.0.113.77` passa a `203.0.113.0`. De imediato, ou após N dias:

```sh
loglem privacy set site --mask-ips-after-days 0
loglem privacy set site --keep-fields request.uri,status
```

- **Guarde só alguns campos JSON.** Todos os outros são descartados.
- **Veja o que guarda:** `loglem privacy report`. Ponha-o no seu registo das atividades de tratamento.

O mascaramento muda só a cópia guardada. A verificação antes de apagar um ficheiro original continua a funcionar. No modo prova, mascarar só é permitido «de imediato» (mais tarde mudaria linhas já seladas).

## Apague os dados de uma pessoa

Alguém pede-lhe que apague os seus dados. Use a página **Dados pessoais**, ou:

```sh
loglem erase --ip 203.0.113.7                                    # só pré-visualização
loglem erase --ip 203.0.113.7 --apply --reason "pedido art. 17 de 17/11/2026, processo 3"
loglem erase --ip 203.0.113.7 --originals --apply --reason "…"   # também dos ficheiros originais
```

O Loglem retira as linhas de **todas** as fontes. O motivo vai para o registo de acessos. O IP nunca é escrito aí.

**Ficheiros originais** (`--originals`, ou a caixa da página). O Loglem reescreve os ficheiros de log terminados sem essas linhas. Só se:

- o ficheiro não for aquele em que um programa escreve,
- ninguém o tiver modificado há 24 horas,
- o ficheiro inteiro já estiver no arquivo do Loglem.

Escreve um ficheiro novo ao lado do antigo, verifica-o e põe-no no lugar. Se algo não bater certo, o original fica. Os ficheiros saltados são listados com o motivo. Repita o comando depois da próxima rotação. Os `.gz` também são reescritos.

**Não** toca:

- nos logs do Podman, Docker, Kubernetes e do journal. Não são ficheiros do Loglem, e o journal não permite retirar entradas soltas. Encurte a sua duração, ou recrie o contentor.
- nas cópias de segurança já feitas (expiram com a sua própria rotação),
- no ficheiro em que um programa escreve, até ser rodado.

Em `--reason` escreva o número de um processo, não um email: um email também é um dado pessoal.

## Exporte os dados de uma pessoa

Alguém pergunta-lhe o que tem sobre ele:

```sh
loglem export --ip 203.0.113.7 --output /sitio/seguro/pedido-3.txt   # --json para JSON
```

Só o proprietário pode ler o ficheiro. Apague-o depois de o enviar.

## Quem consultou

`loglem audit` (ou os ficheiros em `data/audit/`) lista cada acesso, pesquisa, exportação, apagamento e mudança de definições, com utilizador e hora. Os valores pesquisados **não** são escritos, só o tipo de filtro («texto, ip»). As entradas guardam-se 13 meses.

## Disco e cópias de segurança

- O Loglem não cifra os seus ficheiros. Use um disco cifrado.
- `loglem backup` escreve ficheiros em claro. Cifre-os antes de saírem do servidor (por exemplo com restic).
- Não ponha cópias de logs com dados pessoais num armazenamento de onde não possa apagar (como um bucket bloqueado).

## Modo prova

Para uma fonte que tem de provar que as suas linhas não foram alteradas:

```sh
loglem proof enable site --tsa https://freetsa.org/tsr     # --tsa é opcional
loglem proof verify site
```

Cada dia terminado recebe uma impressão (SHA-256) das suas linhas. Dentro vai também a impressão do dia anterior: mudar uma quebra todas as seguintes. Com `--tsa`, uma autoridade externa assina cada impressão com a hora; só vê a impressão.

`loglem proof verify` recalcula cada dia e diz **íntegro**, **ALTERADO** ou **expirado**. Pode verificar também uma cópia: `loglem proof verify --archive copia.db --chain copia.chain`.

O que prova: um dia selado não mudou depois do selo (ou da hora do carimbo). Não prova que as linhas eram verdadeiras quando foram escritas.

As linhas expiram só por dias inteiros, e cada expiração fica escrita no registo.

**Apagar em modo prova.** Ganha o apagamento. O Loglem retira as linhas, **recalcula** a impressão desse dia e de todos os seguintes, e junta uma nota ao registo: quando, porquê, quantas linhas, impressão antiga → nova. `proof verify` mostra a nota. Os carimbos de tempo antigos continuam válidos para a versão antiga. Pedem-se novos para as impressões novas.
