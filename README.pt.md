# Loglem

[English](README.md) | [Italiano](README.it.md) | [Français](README.fr.md) | [Español](README.es.md) | [Português](README.pt.md) | [Deutsch](README.de.md)

**Um só lugar para os registos do seu servidor: onde estão, quanto espaço ocupam e durante quanto tempo ficam guardados.**

O Loglem lê os registos do seu servidor Linux (ficheiros `.log` e contentores Podman), copia-os para um arquivo pesquisável e apaga automaticamente as entradas arquivadas quando termina o período de conservação que escolheu. Gere tudo a partir de uma página web acessível no próprio servidor ou de um menu do terminal navegável pelo teclado, sem editar ficheiros de configuração.

> **Estado: experimental.** Funciona e está testado, mas ainda não há uma versão publicada e algumas partes precisam de ser verificadas de ponta a ponta num servidor real. Veja os [limites](docs/pt/limitations.md).

## Índice

- [Porquê o Loglem](#porquê-o-loglem)
- [O que faz](#o-que-faz)
- [Início rápido](#início-rápido)
- [Instalação](#instalação)
- [Documentação](#documentação)
- [Licença](#licença)

## Porquê o Loglem

Os registos acumulam-se em muitos sítios: uma pasta para o servidor web, outra para a aplicação, a saída de cada contentor. Ninguém sabe onde estão, quanto cresceram nem quando deviam ser apagados.

Com o Loglem:

- **Sabe sempre onde estão os registos.** Uma lista mostra cada fonte, a localização original, a localização do arquivo, o tamanho e o estado da última leitura.
- **Decide durante quanto tempo guarda os registos de cada fonte.** 28 dias, 6 meses ou 2 anos, fonte a fonte. As entradas expiradas são removidas automaticamente: o disco não enche e os dados antigos não ficam por esquecimento.
- **Pode pesquisar.** Texto e intervalos de datas em tudo o que foi recolhido, mesmo após a rotação do ficheiro original ou de o contentor ter sido removido.
- **Não incomoda.** Um único executável de cerca de 4 MB, cerca de 10 MB de RAM em repouso, sem servidor de base de dados, sem conta e sem serviço na nuvem. Os seus registos nunca saem da máquina.
- **É seguro por predefinição.** Os ficheiros originais só são lidos, nunca alterados. Apagar um original é opcional e só se aplica aos ficheiros que entrega explicitamente depois de terminada a escrita.
- **É simples de usar.** Tudo se faz a partir de uma página que só escuta no próprio servidor, ou de um menu de terminal que também funciona por SSH. Há ainda comandos para scripts e automatização.
- **Substitui um monte de scripts.** Recolher, limpar, conservar e pesquisar registos costuma exigir uma mistura de regras de rotação, tarefas agendadas e comandos improvisados. O Loglem faz tudo num só lugar, com as mesmas definições na página web, no terminal e na linha de comandos.
- **Acompanha-o.** Caminhos, contentores e durações são definições, não código: exporte a configuração e importe-a noutra máquina.

## O que faz

1. **Adiciona uma fonte:** uma pasta de ficheiros `.log`, um ficheiro avulso, um contentor Podman ou o registo de acesso do Caddy.
2. **O Loglem recolhe as linhas novas** de 10 em 10 segundos por predefinição e guarda-as num pequeno arquivo SQLite dessa fonte. Lembra-se de onde parou, por isso nada é lido duas vezes.
3. **Pesquisa e verifica** na página web ou no terminal, e vê o estado de cada fonte.
4. **As entradas antigas são removidas** quando ultrapassam a duração definida para essa fonte.
5. **Se algo correr mal,** os erros do Loglem ficam guardados com a respetiva causa e aparecem numa página de Diagnóstico. Uma fonte com falha não interrompe a recolha das outras.

Funciona como serviço em segundo plano (um serviço de utilizador do systemd) ou a pedido. Corre em Linux, sem ambiente gráfico.

## Início rápido

Depois da [instalação](#instalação):

```sh
# Abre o menu guiado do terminal (setas, Enter, Esc)
loglem

# Ou adicione uma pasta de registos na linha de comandos e leia-a uma vez
loglem source add-folder --name web-logs --path /srv/example/logs --days 28
loglem collect
loglem status

# Pesquisar
loglem search --source web-logs --text '/login' --from 2026-11-01 --to 2026-11-16

# Abre a página web (mostra o seu próprio endereço, por exemplo http://127.0.0.1:53142)
loglem web
```

Para continuar a recolher depois de fechar o terminal:

```sh
loglem service install --start
```

O menu do terminal e a página web usam por agora etiquetas em italiano. Detalhes em [Usar o Loglem](docs/pt/usage.md).

## Instalação

Quando for publicada uma versão, descarregue o pacote para a sua máquina no separador **Releases** deste repositório:

| Máquina | Sufixo do pacote |
|---|---|
| Linux Intel/AMD de 64 bits | `x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `aarch64-unknown-linux-gnu.tar.gz` |

Os pacotes precisam de glibc 2.39 ou posterior (por exemplo Ubuntu 24.04).

Antes de instalar, verifique a assinatura com a chave pública do editor (`release-public.pem`), obtida por um canal independente:

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

Tudo fica em `~/Loglem`: o programa e uma pasta `data/` com definições, arquivos e registos de diagnóstico. O instalador não ativa nenhum serviço automaticamente. Para fontes Podman, o Podman tem de estar instalado e estar acessível ao mesmo utilizador que executa o Loglem.

Para o compilar a partir do código-fonte, veja [CONTRIBUTING.md](CONTRIBUTING.md).

## Documentação

A documentação está disponível em: [English](docs/usage.md) | [Italiano](docs/it/usage.md) | [Français](docs/fr/usage.md) | [Español](docs/es/usage.md) | [Português](docs/pt/usage.md) | [Deutsch](docs/de/usage.md)

| Página | Conteúdo |
|---|---|
| [Usar o Loglem](docs/pt/usage.md) | Guia passo a passo: adicionar fontes, pesquisa, conservação, execução como serviço, limpeza, transferência para outra máquina, resolução de problemas e atualizações |
| [Referência de comandos](docs/pt/cli.md) | Todos os comandos |
| [Limites](docs/pt/limitations.md) | O que não faz ou não garante |

## Licença

[MIT](LICENSE).
