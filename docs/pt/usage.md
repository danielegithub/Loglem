# Usar o Loglem

Poucos passos. Todos os comandos estão em [cli.md](cli.md).

## 1. Crie um site e ligue os registos

Na página inicial, escolha **Nuovo sito** e introduza o nome do site ou projeto. Em **Aggiungi un log**, escolha o ficheiro, pasta ou serviço a ler. A página do site mostra a origem, o arquivo, o prazo de conservação e as últimas linhas. **Cerca nei log del sito** limita a pesquisa ao site selecionado.

| Fonte | Comando |
|---|---|
| Pasta de ficheiros `.log` | `loglem source add-folder --name web --path /var/log/myapp` |
| Um ficheiro (e as suas cópias rodadas) | `loglem source add-file --name api --path /var/log/api.log --rotated-pattern 'api.log.*'` |
| Qualquer log, guiado passo a passo (recomendado) | `loglem add` |
| Contentor Podman | `loglem source add-podman --name db --container db` |
| Contentor Docker | `loglem source add-docker --name db --container db` |
| Pods Kubernetes | `loglem source add-kubernetes --name shop --namespace shop --selector app=web` |
| Journal do Linux | `loglem source add-journal --name ssh --identifier sshd` |

`loglem discover` procura os logs que os programas conhecidos indicam na sua configuração (ver `src/recipes/`). A página **Adicionar** fá-lo por si e explica porque propõe cada log.

Os ficheiros comprimidos `.gz` também são lidos. O Loglem importa o histórico uma só vez e nunca duplica uma linha.

Escolhe o site ou projeto ao adicionar uma fonte. Acrescenta `--site example.com` a qualquer comando `source add-*`. Para mover uma fonte existente, usa `loglem source assign-site myapp example.com`. A página inicial agrupa os registos por site. As fontes do journal sem site começam no grupo do sistema. Durante a migração, o Loglem guarda a configuração anterior em `config.json.bak-AAAAMMDD`.

Abra um site e expanda **Copie** para ver os caminhos, tamanhos e responsáveis pela limpeza. Um ficheiro partilhado conta uma única vez no total do site. O Loglem compara até 200 linhas recentes por origem. Se mais de metade da amostra maior coincidir, pode suspender uma das recolhas. A pesquisa externa verifica a pedido as pastas dos registos, `/var/log`, `/srv`, o journal local, Podman e Docker durante até oito segundos. O total de bytes idênticos inclui apenas ficheiros distintos verificados por inteiro.

## 2. Pesquise e siga em direto

Abra **Explorar logs**. Escreva um texto, escolha datas, ou abra **Filtros avançados**: gravidade, endereço IP, caminho, código de resposta.

As linhas novas aparecem em cima sozinhas. Num terminal:

```sh
loglem search --text login --ignore-case --status 404 --source site
loglem tail --source site --ip 203.0.113.7
```

Para estatísticas de um log de acesso (pedidos por dia, páginas mais vistas, códigos, IPs mais frequentes): `loglem stats site`, ou o botão **Estatísticas** na ficha da fonte.

## 3. Escolha por quanto tempo guardar

Cada fonte tem uma duração em dias, meses ou anos. As cópias antigas são apagadas por si. Os ficheiros originais não são tocados.

```sh
loglem source set site --days 28
```

## 4. Mantenha o Loglem a funcionar

`loglem serve` lê os logs e serve a página web. A página mantém o mesmo endereço, mesmo depois de um reinício.

```sh
loglem service install --start    # Linux: systemd. macOS: launchd.
loglem service status
loglem check                      # código de saída 1 se algo estiver mal
```

O serviço é uma unidade de utilizador do systemd. Num servidor, execute uma vez `loginctl enable-linger UTILIZADOR` para que arranque com o computador e continue depois de fechar o SSH. `loglem service install` avisa quando é preciso.

O serviço reinicia sozinho depois de uma falha. Numa paragem normal, lê cada fonte uma última vez. O que acontece enquanto o Loglem está desligado: [stopping.md](stopping.md).

O serviço também faz a verificação automática (a cada 5 minutos, sem cron). A página Sites e **Sistema → Serviço** mostram se está a correr, desde quando e quantas vezes reiniciou. Para o retirar: `loglem service uninstall`. Os dados ficam.

## 5. Deixe o Loglem apagar os ficheiros originais

Desligado por defeito. O Loglem nunca toca no ficheiro em que um programa está a escrever. Apaga só ficheiros terminados, depois desta verificação:

1. O Loglem copia o ficheiro para o seu arquivo.
2. Compara a cópia com o original, byte a byte. Uma linha em falta ou expirada pára a verificação.
3. Espera que ninguém tenha modificado o ficheiro durante **24 horas**.
4. Só então apaga o original.

Se um passo falhar, o original fica.

**Ficheiros rodados.** As novas fontes conservam os originais. Para apagar os ficheiros rodados após a verificação, ative a limpeza nas definições da fonte.

**Outros programas.** Faça o programa pôr os ficheiros *terminados* numa pasta à parte e depois:

```sh
loglem source configure web --completed-dir /var/log/done --handoff-confirmed
```

Nunca indique uma pasta onde um programa ainda escreve.

**Quem apaga os originais?** Um só sítio: a caixa de limpeza nas definições da fonte. O Loglem lê as regras do logrotate e diz se já existe uma rotação. Se o logrotate (ou o programa) já roda e apaga os ficheiros, deixe a limpeza do Loglem **desligada**: dois sistemas a apagar os mesmos ficheiros não servem e escondem os erros. O Loglem avisa se estão os dois ligados, se os originais duram mais do que a sua cópia e se os ficheiros rodados têm nomes que ele não lê.

## 6. Abra a página a partir de outro computador

Desligado por defeito: a página responde só neste computador. Para a abrir noutro sítio:

```sh
loglem access user-add anna --role admin     # reader = só pode ver
loglem access address 10.0.0.1               # um endereço deste servidor, por exemplo o da VPN
```

Reinicie o Loglem. Cinco palavras-passe erradas bloqueiam esse computador durante 15 minutos. A ligação **não é cifrada**: use uma VPN, ou um proxy inverso com HTTPS:

```sh
loglem access proxy --trust 127.0.0.1 --base-path /loglem
```

Mostra um bloco para o Caddyfile. Para seguir em direto a partir de outro computador: `loglem tail --server http://10.0.0.1:8790 --user anna`.

## 7. Alertas e cópia de segurança

```sh
loglem alerts add-pattern acessos-falhados --text "Failed password" --count 5 --minutes 10
loglem alerts command 'echo "$LOGLEM_ALERTS" | mail -s Loglem me@example.com'
loglem backup /backups/loglem-2026-11-17
loglem backup-verify /backups/loglem-2026-11-17
```

Os alertas aparecem na página, no `loglem check` e em `data/health.json`. **Marcar como visto** esconde um até voltar. Uma cópia de segurança guarda também as linhas entretanto expiradas.

### Estado e avisos por e-mail

O Loglem verifica-se sozinho a cada 5 minutos (mude em **Sistema → Verificação automática**, ou com `loglem alerts monitor --every 10`). A página mostra «Tudo em ordem, verificado às 14:35» ou o que está mal.

Os avisos por e-mail são opcionais e usam o [Resend](https://resend.com):

```sh
loglem alerts email-set --from noreply@example.com --to admin@example.com   # pede a chave
loglem alerts email-test
loglem alerts email-off            # apaga também a chave
```

Crie uma chave **só de envio** limitada ao seu domínio e verifique o domínio no Resend. O remetente pode ser `noreply@` num subdomínio. As respostas não são lidas. O Loglem envia um e-mail quando um problema abre, outro quando fecha e um lembrete se continuar aberto mais de 24 horas. Os e-mails não têm linhas de log. No máximo 6 por hora. A chave fica num ficheiro privado (`secrets/resend.key`): nunca nas definições, exportações, cópias de segurança ou logs.

## 8. Onde estão os seus dados

```text
data/
├── config.json     fontes e definições
├── archives/       logs guardados, uma base por fonte
├── audit/          quem pesquisou, exportou ou apagou
├── proof/          registos dos selos (modo prova)
└── diagnostics/    os erros do Loglem
```

Para outro lugar use `--data-dir /caminho/absoluto` ou `LOGLEM_DATA_DIR`. Mova as definições com `loglem config export` e `config import`. Mova os logs parando o Loglem e copiando `data/`.

Os arquivos de versões antigas são convertidos no arranque. O texto guardado fica igual, o ficheiro fica mais pequeno.

## 9. Se algo correr mal

- Uma fonte mostra um erro: as outras continuam. Abra a sua ficha, ou `loglem status`.
- Os erros do Loglem: **Problemas** na página, ou `loglem diagnostics`.
- Porta ocupada: `loglem config web-port 8790`, e reinicie.
- Verifique o programa: `loglem self-check`.

## 10. Atualize

Carregue em **Verificar** na página **Sistema**, ou:

```sh
loglem update check
loglem update install
```

Só se instalam versões assinadas (Linux). `data/` é mantida. O programa antigo fica como `loglem.previous`.

## 11. Num contentor

```sh
podman build -t loglem -f Containerfile .
podman run -d --name loglem -p 127.0.0.1:8790:8790 \
  -e LOGLEM_ADMIN_USER=anna -e LOGLEM_ADMIN_PASSWORD='uma palavra-passe longa' \
  -v loglem-data:/data -v /var/log/myapp:/logs/myapp:ro loglem
```

Abra `http://127.0.0.1:8790` e adicione `/logs/myapp` como fonte pasta. Os dados ficam no volume `loglem-data`. O Podman, o Docker e o journal do anfitrião não são alcançáveis a partir de dentro: para esses, use o Loglem no anfitrião, ou monte os ficheiros de log em `/logs`. Para limpar os ficheiros originais, monte a pasta sem `:ro`.

Dados pessoais, ferramentas RGPD e modo prova: [privacy.md](privacy.md).
