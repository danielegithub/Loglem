# Limites

O que o Loglem não faz, ou não garante, nesta versão.

**O que é suportado**

- Apenas Linux. O Windows não é suportado; no macOS não há serviço automático (use `loglem web`).
- Fontes: ficheiros de texto `.log` (UTF-8), contentores Podman e registos de acesso do Caddy. Os ficheiros comprimidos (`.gz`) não são lidos. Docker, Kubernetes, Nginx e o journal do systemd ainda não são suportados.
- Linhas até 1 MiB.
- A pesquisa permite filtrar por texto exato, com distinção entre maiúsculas e minúsculas, e por intervalo de datas. Ainda não há filtros por gravidade, endereço IP, caminho ou código de resposta.
- A página web só se abre a partir da máquina onde o Loglem corre (use um túnel SSH para um servidor remoto).

**Segurança dos dados**

- Os registos do Podman são lidos por instantâneos. Se um contentor for apagado antes de o Loglem o ler, ou se o Loglem ficar parado muito tempo, podem perder-se linhas.
- As entradas só expiram enquanto o Loglem está a correr (ou quando executa `loglem prune --apply`).
- Apagar uma entrada do arquivo do Loglem não a apaga dos instantâneos ou cópias de segurança do sistema de ficheiros que já a copiaram.
- Os arquivos são bases SQLite e têm de estar num disco local, não numa pasta de rede. Não os copie enquanto o Loglem está a correr; pare-o primeiro.
- Se o disco encher, os originais ainda não arquivados são mantidos; o Loglem não pode libertar espaço nem impedir outros programas de escrever.
- O Loglem não consegue saber sozinho que um programa terminou com um ficheiro. A sua limpeza espera 24 horas e nunca toca no ficheiro em que um programa está a escrever, mas ative-a apenas para pastas e registos do Caddy configurados como descrito no guia.
- Os arquivos não são à prova de adulteração: o Loglem não garante o valor probatório dos registos.

**Testado e ainda não testado**

- Testado com Podman real e um navegador real: adicionar fontes pela página, pesquisa, reinício sem linhas perdidas nem repetidas, expiração e rotação simulada ao estilo do Caddy.
- Ainda não testados: o serviço systemd num Linux real, o Caddy real e o regresso à versão anterior após uma atualização falhada.
- Ainda não há uma versão publicada.
