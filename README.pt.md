# Loglem

[English](README.md) | [Italiano](README.it.md) | [Français](README.fr.md) | [Español](README.es.md) | [Português](README.pt.md) | [Deutsch](README.de.md)

**Um só lugar para os logs do seu servidor: onde estão, quanto espaço ocupam e por quanto tempo ficam.**

O Loglem lê os seus logs, copia-os para um arquivo onde se pode pesquisar e apaga as cópias quando passa o tempo que escolheu. Usa uma página web ou um menu no terminal. Sem ficheiros de configuração.

> **Estado: experimental.** Funciona e está testado no macOS. Ainda não há versão publicada, e algumas partes não estão testadas num Linux real. Veja os [limites](docs/pt/limitations.md).

## Índice

- [Porquê o Loglem](#porquê-o-loglem)
- [O que lê](#o-que-lê)
- [Limpeza segura dos ficheiros originais](#limpeza-segura-dos-ficheiros-originais)
- [Começar](#começar)
- [Instalação](#instalação)
- [Documentação](#documentação)
- [Licença](#licença)

## Porquê o Loglem

Os logs acumulam-se em muitos sítios. Ninguém se lembra de onde estão, de que tamanho são ou quando apagá-los.

- **Vê tudo.** Uma lista: cada fonte, onde está, o tamanho, o estado.
- **Escolhe por quanto tempo.** 28 dias, 6 meses, 2 anos, por fonte. As cópias antigas desaparecem sozinhas.
- **Pode pesquisar.** Texto, datas, IP, caminho, código de resposta. Mesmo depois de o ficheiro ser rodado ou o contentor removido. As linhas novas aparecem em direto.
- **Fica pequeno.** Um programa de cerca de 4 MB. Cerca de 5 MB de RAM (medidos em Linux). O arquivo ocupa cerca de um décimo dos logs originais. Sem servidor de base de dados, sem conta, sem nuvem. Os logs ficam na sua máquina. [Medições (em inglês)](docs/performance.md).
- **É seguro por defeito.** Os ficheiros originais só são lidos. Apagá-los é opcional e verificado (veja abaixo).
- **É simples.** Uma página web, um menu de terminal que funciona por SSH e comandos para scripts.
- **Mostra o que correu mal.** Uma fonte parada, uma falha nos logs, um disco cheio: vê-o, e o `loglem check` avisa o seu monitor.
- **Ajuda com os dados pessoais.** Mascare IPs, apague ou exporte as linhas de uma pessoa, veja quem consultou os logs. Veja [privacidade](docs/pt/privacy.md).

## O que lê

| Fonte | Notas |
|---|---|
| Pastas e ficheiros de texto `.log` | Também ficheiros rodados, também `.gz` (o histórico é importado uma vez, nunca duplicado). |
| Servidores web, bases de dados, correio, SSH, cron… | Reconhecidos pelas suas receitas em `src/recipes/`: Nginx, Apache, Caddy, Traefik, HAProxy, PostgreSQL, MySQL e outros. Adicionar um é um só ficheiro. |
| Podman, Docker | Um contentor, ou pelo nome através do journal. |
| Kubernetes | Um pod, ou todos os pods de uma aplicação. |
| Journal do Linux | Tudo, ou alguns serviços, programas ou níveis de gravidade. |

## Limpeza segura dos ficheiros originais

Desligada por defeito. Se a ativar para uma fonte, o Loglem apaga um ficheiro terminado só depois desta verificação:

1. **Copia** o ficheiro para o seu arquivo.
2. **Compara a cópia com o original, byte a byte.** Se faltar uma linha, tiver expirado ou tiver mudado, pára.
3. Espera que ninguém tenha modificado o ficheiro durante **24 horas**.
4. Só então apaga o original.

O ficheiro em que um programa ainda escreve nunca é tocado. Se um passo falhar, o original fica onde está. Vale para os ficheiros rodados de qualquer programa e para pastas de ficheiros terminados.

## Começar

Depois da [instalação](#instalação):

```sh
loglem                                  # menu no terminal
loglem discover                         # procura os logs na configuração dos programas conhecidos
loglem source add-folder --name web --path /srv/example/logs --days 28
loglem collect
loglem search --source web --text '/login' --from 2026-11-01
loglem web                              # lê e abre a página web (mostra o endereço)
```

Para continuar depois de fechar o terminal (Linux: systemd, macOS: launchd):

```sh
loglem service install --start
```

O menu e a página web estão em italiano. Mais em [Usar o Loglem](docs/pt/usage.md).

## Instalação

Quando houver uma versão publicada, descarregue o pacote para a sua máquina no separador **Releases**:

| Máquina | Fim do nome do pacote |
|---|---|
| Linux Intel/AMD 64 bits | `x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `aarch64-unknown-linux-gnu.tar.gz` |

Os pacotes precisam de glibc 2.39 ou superior (por exemplo Ubuntu 24.04). As atualizações, os carimbos temporais e `loglem tail --server` usam o comando `curl` do sistema.

Verifique a assinatura com a chave pública do editor (`release-public.pem`), obtida por um canal independente:

```sh
openssl pkeyutl -verify -pubin -inkey release-public.pem -rawin -in SHA256SUMS -sigfile SHA256SUMS.sig
sha256sum --check --ignore-missing SHA256SUMS
```

Depois instale:

```sh
tar xzf loglem-*.tar.gz
cd loglem-*
sh install.sh
~/Loglem/loglem
```

Tudo fica em `~/Loglem`: o programa e uma pasta `data/`. O instalador não inicia nenhum serviço. Para o compilar você mesmo, veja [CONTRIBUTING.md](CONTRIBUTING.md).

**Num contentor.** Construa-o com `podman build -t loglem -f Containerfile .` e execute-o montando uma pasta de logs. Veja [Usar o Loglem](docs/pt/usage.md#11-num-contentor).

## Documentação

[English](docs/usage.md) | [Italiano](docs/it/usage.md) | [Français](docs/fr/usage.md) | [Español](docs/es/usage.md) | [Português](docs/pt/usage.md) | [Deutsch](docs/de/usage.md)

| Página | O que tem |
|---|---|
| [Usar o Loglem](docs/pt/usage.md) | Poucos passos para tudo |
| [Comandos](docs/pt/cli.md) | Todos os comandos |
| [Dados pessoais e modo prova](docs/pt/privacy.md) | Ferramentas RGPD, apagar, exportar, registo de acessos, dias selados |
| [Se o Loglem parar](docs/pt/stopping.md) | O que perde e o que o Loglem faz |
| [Limites](docs/pt/limitations.md) | O que não faz ou não promete |

## Licença

[MIT](LICENSE).
