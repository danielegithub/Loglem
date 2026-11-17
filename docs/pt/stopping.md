# Se o Loglem parar

O Loglem pode estar desligado: uma falha, um reinício, uma atualização, um erro. Isto é o que perde.

| Fonte | Enquanto o Loglem está desligado | O que se perde |
|---|---|---|
| Ficheiros, Caddy, Nginx | Os programas continuam a escrever e a rodar. | Nada, até o programa apagar por si os ficheiros antigos (`roll_keep`, `roll_keep_for`, logrotate). |
| Podman, Docker | O programa dos contentores guarda os logs. | As linhas que apagou, e todas se o contentor foi removido ou recriado. |
| Kubernetes | Os pods guardam os logs. | As linhas de um pod substituído. Siga `app=nome`, não um só pod. |
| Journal do Linux | O journal guarda as entradas. | As entradas que o journal apagou por espaço ou idade. |

Enquanto o Loglem está desligado, as linhas expiradas não são apagadas e os ficheiros originais não são limpos.

## O que o Loglem faz

- **Quando volta a arrancar** lê a partir do ponto guardado. A ficha diz «Retomado após 3 horas parado: N linhas recuperadas».
- **As falhas veem-se.** Se podem faltar linhas, dizem-no a ficha, o aviso e o `loglem check`, com um início e um fim estimados.
- **Paragem normal.** Numa paragem normal (`systemctl stop`, uma atualização) lê cada fonte uma última vez, no máximo 30 segundos.
- **Falha.** No arranque seguinte diz que a última paragem não foi limpa. Retoma a partir do último ponto guardado. Uma linha guarda-se junto com a sua posição de leitura, por isso nada é lido duas vezes.
- **Reinício automático.** O serviço reinicia depois de uma falha, no máximo 10 vezes em 5 minutos.
- **Alerta.** `loglem check` falha se o Loglem estiver desligado ou uma fonte não for lida há 15 minutos. Lance-o a partir do cron ou de um monitor.
- **Margem do Caddy.** O Loglem avisa se `roll_keep_for` estiver abaixo de 7 dias, ou se os ficheiros rodados por ler estiverem perto de `roll_keep`.

## Para ficar mais seguro

- Logs de contentores: siga o contentor pelo nome através do journal, se escrever lá (a página oferece-o). Sobrevive a um novo lançamento.
- Kubernetes: use `--selector app=nome`.
- Mantenha `roll_keep_for` do Caddy em 30 dias ou mais.
- Lance `loglem check` de poucos em poucos minutos.
