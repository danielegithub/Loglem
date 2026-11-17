# Limites

O que o Loglem não faz, ou não promete.

## Plataformas

- Linux e macOS. O serviço é systemd (Linux) ou launchd (macOS). Atualizações assinadas: só Linux.
- Windows: o código está escrito para ser portável, mas não foi compilado nem testado.
- Ainda não há versão publicada.
- Num contentor, o Loglem lê só os ficheiros de log que montar. Podman, Docker, Kubernetes e o journal do anfitrião precisam do Loglem no anfitrião.

## O que lê

- Linhas de texto UTF-8 até 1 MiB. Outras codificações não são lidas.
- Os filtros de gravidade, código e caminho leem o texto da linha. Conhecem o Caddy (JSON), Nginx e Apache (combined) e o journal. Outros formatos só se encontram pesquisando o texto.
- As estatísticas funcionam com logs do Caddy em JSON e do Nginx/Apache em formato combined.
- Ainda não há expressões regulares.

## O que se pode perder

- Podman, Docker e Kubernetes guardam os logs só enquanto o contentor ou o pod existe. Se for removido enquanto o Loglem está desligado, essas linhas perdem-se. O Loglem mostra uma **falha** na ficha.
- Para o Kubernetes, siga a aplicação por etiqueta (`app=web`). Um pod isolado desaparece em cada lançamento.
- Se o Loglem estiver desligado, as linhas não expiram e nada é limpo. Veja [stopping.md](stopping.md).
- Disco cheio: os ficheiros ainda não arquivados são mantidos. O Loglem não consegue libertar espaço.

## Segurança dos dados

- Os arquivos são ficheiros SQLite num disco local, não numa pasta de rede. Pare o Loglem antes de os copiar à mão, ou use `loglem backup`.
- Apagar uma linha no Loglem não a apaga das cópias de segurança nem dos ficheiros originais.
- O Loglem não cifra os dados no disco. Use um disco cifrado e cópias cifradas.
- A ligação à página web não é cifrada. Use uma VPN ou um proxy inverso com HTTPS.
- O Loglem não consegue saber se um programa terminou com um ficheiro. A limpeza espera 24 horas e nunca toca no ficheiro em uso.
- O modo prova mostra que um dia selado não mudou. Não prova que as linhas eram verdadeiras. Veja [privacy.md](privacy.md).

## O que foi testado

Testado no macOS com Podman, journald (num contentor Linux), Caddy, Chrome e uma autoridade de carimbo de tempo real: adicionar fontes pela página, pesquisa, direto, acesso com palavra-passe, reinício sem linhas perdidas nem duplicadas, expiração, rotação do Caddy, proxy inverso, cópia e restauro, modo prova, apagar.

Não testado: o serviço systemd num Linux real, um cluster Kubernetes real (usou-se um `kubectl` falso), o agente launchd do macOS, o Windows e uma atualização a partir de uma versão publicada.
