<h1 align="center">
  <img src="public/mangodisk.svg" width="40" alt="Ícone do aplicativo MangoDisk"> MangoDisk
</h1>

<p align="center">Limpeza de disco, análise de armazenamento e proteção da privacidade para <b>macOS</b>, <b>Windows</b> e <b>Linux</b></p>

<p align="center">
  Português (Brasil) · <a href="README.zh-CN.md">简体中文</a> · <a href="README.zh-TW.md">繁體中文</a> · <a href="README.ja.md">日本語</a> · <a href="README.ko.md">한국어</a> · <a href="README.ru.md">Русский</a>
</p>

<p align="center">
  <a href="https://github.com/harry0703/MangoDisk/releases/latest"><img alt="Versão mais recente" src="https://img.shields.io/github/v/release/harry0703/MangoDisk?display_name=tag&sort=semver"></a>
  <img alt="macOS compatível" src="https://img.shields.io/badge/macOS-supported-111827?logo=apple&logoColor=white">
  <img alt="Windows compatível" src="https://img.shields.io/badge/Windows-supported-2563eb?logo=windows&logoColor=white">
  <img alt="Linux compatível" src="https://img.shields.io/badge/Linux-supported-f59e0b?logo=linux&logoColor=white">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24c8db?logo=tauri&logoColor=white">
  <img alt="Núcleo em Rust" src="https://img.shields.io/badge/core-Rust-b7410e?logo=rust&logoColor=white">
</p>

<p align="center">
  <a href="https://mangodisk.app/">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://assets.mangodisk.app/images/readme/en-dark.jpg">
      <source media="(prefers-color-scheme: light)" srcset="https://assets.mangodisk.app/images/readme/en-light.jpg">
      <img src="https://assets.mangodisk.app/images/readme/en-light.jpg" width="1200" alt="Limpeza de disco, análise de armazenamento, proteção da privacidade e otimização do sistema no MangoDisk">
    </picture>
  </a>
</p>

## O que o MangoDisk faz

> **Armazenamento**

### 1. Limpeza profunda

Encontre, em uma única análise, itens que podem ser limpos no sistema, nos aplicativos, nas ferramentas de desenvolvimento e nos projetos locais. O MangoDisk reúne os resultados pelo espaço que pode ser recuperado:

- **Caches do sistema e do usuário**: recupere o espaço ocupado por arquivos temporários, dados de diagnóstico e caches que podem ser recriados.
- **Caches de aplicativos**: evite que caches, logs, pacotes de atualização e arquivos temporários consumam cada vez mais espaço.
- **Dados de navegadores**: recupere o espaço usado por dados temporários e em cache do Chrome, Edge, Firefox, Brave, Arc, Opera e outros navegadores.
- **Ferramentas de desenvolvimento e Xcode**: recupere o espaço ocupado por gerenciadores de pacotes, IDEs, caches de compilação e dados de desenvolvimento do Xcode.
- **Caches de contêineres**: libere o espaço usado por caches de build inativos e dados recriáveis do Docker e de outras ferramentas de contêineres.
- **Artefatos de build de projetos**: recupere o espaço usado por dependências recriáveis, caches e diretórios de build de projetos Node.js, Rust, Gradle, Swift, Python, .NET, Godot, CMake e outros.
- **Modelos e caches de IA**: identifique modelos locais de IA grandes, caches de download e arquivos temporários de transferência.
- **Otimização de aplicativos**: reduza o espaço ocupado por aplicativos compatíveis sem afetar seu uso normal.

As recomendações ajudam você a tomar decisões seguras com rapidez. Também é possível revisar cada item e conferir a estimativa de espaço recuperável antes de limpar, mantendo o controle sobre a operação.

### 2. Limpeza de arquivos grandes

Encontre rapidamente os maiores arquivos e recupere o espaço ocupado por instaladores antigos, vídeos, arquivos compactados e outros conteúdos grandes, sem precisar vasculhar pasta por pasta.

### 3. Limpeza de arquivos duplicados

Recupere o espaço ocupado por cópias duplicadas sem considerar dois arquivos iguais apenas porque têm o mesmo nome. A seleção automática mantém pelo menos um arquivo em cada grupo para tornar a limpeza mais segura.

### 4. Análise do espaço em disco

Veja de imediato o que ocupa espaço no disco. Alterne entre um **mapa de árvore** e um **gráfico radial**, escolha quantos níveis deseja exibir e explore a estrutura das pastas. Navegue pelas pastas ao lado da lista de arquivos para encontrar os maiores itens e decidir o que limpar.

> **Privacidade e segurança**

### 5. Limpeza de privacidade

Remova históricos de navegação e pesquisa, cookies, itens recentes e dados da área de transferência que permanecem no computador. Limpe rastros deixados por navegadores, aplicativos e pelo sistema para reduzir a exposição das suas atividades.

> **Ferramentas do sistema**

### 6. Desinstalação e limpeza de aplicativos

Desinstale aplicativos e remova caches, configurações e resíduos relacionados para recuperar o espaço ocupado. Arquivos que possam ser pessoais são tratados com cautela para reduzir o risco de exclusão acidental.

### 7. Gerenciamento de itens de inicialização

Reduza atrasos desnecessários na inicialização e o uso de recursos em segundo plano. Você pode reativar os itens quando precisar deles novamente.

### 8. Otimização do sistema

Ajuste configurações que atrapalham o uso ou deixam o sistema mais lento. Equilibre desempenho, privacidade e preferências pessoais para usar o computador com mais facilidade.

### 9. Manutenção do sistema

Corrija problemas comuns, como resultados de pesquisa ausentes, ícones incorretos, falta de som ou falhas de conexão, sem procurar soluções nem digitar comandos complexos.

> **Atividade**

### 10. Histórico de operações

Consulte o registro de cada limpeza e alteração no sistema. Veja quanto espaço foi recuperado, quais operações foram concluídas e se alguma ainda precisa da sua atenção.

## Uso de recursos e gerenciamento de memória

> Disponível desde a versão 1.1.1

Confira o uso de CPU e memória, a velocidade da rede e a atividade do disco. Veja quais aplicativos consomem mais memória e libere memória com um clique quando os recursos estiverem escassos.

Mantenha essas informações na barra de menus, na barra de tarefas ou na bandeja do sistema, sem precisar abrir a janela principal.

## Explicações por IA

> Disponível desde a versão 1.1.0

Não sabe para que serve um item ou o que pode acontecer se alterá-lo? As explicações por IA usam a descrição do item e os resultados da análise atual para mostrar sua função e o que considerar antes de agir.

Consulte as explicações diretamente nos itens de Limpeza profunda (regras integradas), Limpeza de privacidade, Itens de inicialização, Otimização do sistema e Manutenção do sistema.

As versões oficiais incluem explicações gratuitas diárias e permitem conectar seu próprio serviço de IA. A IA oferece orientações; você decide quais ações executar.

## Segurança e regras

> [!IMPORTANT]
> **O MangoDisk prioriza a segurança dos dados.**
> Regras de limpeza e otimizações do sistema só são disponibilizadas após a definição clara dos seus limites de segurança e a validação em sistemas reais.

Por padrão, o MangoDisk faz análises somente de leitura. Antes de iniciar uma limpeza, exclusão, desinstalação ou alteração de configuração, você pode revisar e confirmar exatamente o que acontecerá. Os resultados são salvos no Histórico de operações.

A Otimização do sistema usa apenas configurações integradas e validadas. Ela não aceita caminhos arbitrários do Registro, comandos de terminal nem scripts. O MangoDisk verifica cada configuração após alterá-la e destaca itens de alto impacto e mudanças que exigem acesso de administrador ou reinicialização.

O MangoDisk mantém suas próprias regras de limpeza. Projetos de terceiros podem servir como ponto de partida para pesquisa, mas uma regra só é aceita após a verificação de fontes confiáveis, limites seguros e comportamento em sistemas reais. Regras sem limites claros de segurança são excluídas.

A biblioteca completa de regras e seu histórico de revisões podem ser consultados em [regras de limpeza do MangoDisk](https://github.com/harry0703/MangoDisk/tree/main/src-tauri/crates/mangodisk-core/rules).

## Capturas de tela

<p align="center">
  <strong>Limpeza profunda</strong><br>
  <sub>Encontre itens que podem ser limpos no sistema, em aplicativos, ferramentas de desenvolvimento e projetos</sub>
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://assets.mangodisk.app/images/screenshots/en/dark-01-deep-cleanup.jpg">
    <source media="(prefers-color-scheme: light)" srcset="https://assets.mangodisk.app/images/screenshots/en/light-01-deep-cleanup.jpg">
    <img src="https://assets.mangodisk.app/images/screenshots/en/light-01-deep-cleanup.jpg" width="1200" alt="Tela de Limpeza profunda do MangoDisk">
  </picture>
</p>

<table>
  <tr>
    <td width="50%" align="center">
      <strong>Limpeza de arquivos grandes</strong><br>
      <sub>Encontre os arquivos que mais ocupam espaço sem vasculhar pastas</sub><br><br>
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="https://assets.mangodisk.app/images/screenshots/en/dark-02-large-file-cleanup.jpg">
        <source media="(prefers-color-scheme: light)" srcset="https://assets.mangodisk.app/images/screenshots/en/light-02-large-file-cleanup.jpg">
        <img src="https://assets.mangodisk.app/images/screenshots/en/light-02-large-file-cleanup.jpg" width="100%" alt="Tela de Limpeza de arquivos grandes do MangoDisk">
      </picture>
    </td>
    <td width="50%" align="center">
      <strong>Limpeza de arquivos duplicados</strong><br>
      <sub>Remova duplicatas exatas com segurança, mantendo pelo menos uma cópia</sub><br><br>
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="https://assets.mangodisk.app/images/screenshots/en/dark-03-duplicate-cleanup.jpg">
        <source media="(prefers-color-scheme: light)" srcset="https://assets.mangodisk.app/images/screenshots/en/light-03-duplicate-cleanup.jpg">
        <img src="https://assets.mangodisk.app/images/screenshots/en/light-03-duplicate-cleanup.jpg" width="100%" alt="Tela de Limpeza de arquivos duplicados do MangoDisk">
      </picture>
    </td>
  </tr>
  <tr>
    <td width="50%" align="center">
      <strong>Análise do espaço em disco</strong><br>
      <sub>Veja o que ocupa espaço e encontre rapidamente os maiores arquivos e pastas</sub><br><br>
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="https://assets.mangodisk.app/images/screenshots/en/dark-05-disk-space-analysis.jpg">
        <source media="(prefers-color-scheme: light)" srcset="https://assets.mangodisk.app/images/screenshots/en/light-05-disk-space-analysis.jpg">
        <img src="https://assets.mangodisk.app/images/screenshots/en/light-05-disk-space-analysis.jpg" width="100%" alt="Tela de Análise do espaço em disco do MangoDisk">
      </picture>
    </td>
    <td width="50%" align="center">
      <strong>Gerenciamento de itens de inicialização</strong><br>
      <sub>Reduza programas desnecessários na inicialização e a atividade em segundo plano</sub><br><br>
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="https://assets.mangodisk.app/images/screenshots/en/dark-06-startup-items.jpg">
        <source media="(prefers-color-scheme: light)" srcset="https://assets.mangodisk.app/images/screenshots/en/light-06-startup-items.jpg">
        <img src="https://assets.mangodisk.app/images/screenshots/en/light-06-startup-items.jpg" width="100%" alt="Tela de Itens de inicialização do MangoDisk">
      </picture>
    </td>
  </tr>
  <tr>
    <td width="50%" align="center">
      <strong>Desinstalação e limpeza de aplicativos</strong><br>
      <sub>Desinstale aplicativos e remova resíduos relacionados para recuperar espaço</sub><br><br>
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="https://assets.mangodisk.app/images/screenshots/en/dark-04-app-uninstaller.jpg">
        <source media="(prefers-color-scheme: light)" srcset="https://assets.mangodisk.app/images/screenshots/en/light-04-app-uninstaller.jpg">
        <img src="https://assets.mangodisk.app/images/screenshots/en/light-04-app-uninstaller.jpg" width="100%" alt="Tela de Desinstalação de aplicativos do MangoDisk">
      </picture>
    </td>
    <td width="50%" align="center">
      <strong>Otimização do sistema</strong><br>
      <sub>Ajuste desempenho, privacidade e usabilidade com um clique</sub><br><br>
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="https://assets.mangodisk.app/images/screenshots/en/dark-07-system-optimization.jpg">
        <source media="(prefers-color-scheme: light)" srcset="https://assets.mangodisk.app/images/screenshots/en/light-07-system-optimization.jpg">
        <img src="https://assets.mangodisk.app/images/screenshots/en/light-07-system-optimization.jpg" width="100%" alt="Tela de Otimização do sistema do MangoDisk">
      </picture>
    </td>
  </tr>
  <tr>
    <td width="50%" align="center">
      <strong>Manutenção do sistema</strong><br>
      <sub>Corrija problemas comuns e volte a usar o computador normalmente</sub><br><br>
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="https://assets.mangodisk.app/images/screenshots/en/dark-08-system-maintenance.jpg">
        <source media="(prefers-color-scheme: light)" srcset="https://assets.mangodisk.app/images/screenshots/en/light-08-system-maintenance.jpg">
        <img src="https://assets.mangodisk.app/images/screenshots/en/light-08-system-maintenance.jpg" width="100%" alt="Tela de Manutenção do sistema do MangoDisk">
      </picture>
    </td>
    <td width="50%" align="center">
      <strong>Limpeza de privacidade</strong><br>
      <sub>Deixe menos rastros de atividade e proteja sua privacidade</sub><br><br>
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="https://assets.mangodisk.app/images/screenshots/en/dark-09-privacy-cleanup.jpg">
        <source media="(prefers-color-scheme: light)" srcset="https://assets.mangodisk.app/images/screenshots/en/light-09-privacy-cleanup.jpg">
        <img src="https://assets.mangodisk.app/images/screenshots/en/light-09-privacy-cleanup.jpg" width="100%" alt="Tela de Limpeza de privacidade do MangoDisk">
      </picture>
    </td>
  </tr>
</table>

## Antes de começar

> [!CAUTION]
>
> 1. Operações de limpeza, exclusão permanente e desinstalação podem ser irreversíveis. Revise os itens selecionados e mantenha cópias de segurança dos dados importantes.
> 2. Antes de executar uma manutenção ou alterar um item de inicialização ou uma configuração do sistema, entenda sua finalidade e seu impacto.
> 3. Algumas otimizações podem afetar a segurança, a privacidade, a duração da bateria ou o funcionamento das atualizações.

## Aplicativo para desktop

Baixe o MangoDisk na [página oficial](https://mangodisk.app/download) ou em [GitHub Releases](https://github.com/harry0703/MangoDisk/releases/latest) e siga as instruções para seu sistema operacional.

### macOS

**Requisitos:** macOS Monterey 12.5 ou posterior.

**Instalação pelo Homebrew:**

```sh
brew install --cask harry0703/tap/mangodisk
```

**Instalação manual:** baixe o DMG na [página oficial](https://mangodisk.app/download), abra-o e arraste o MangoDisk para a pasta Aplicativos.

### Windows

**Requisitos:** Windows 10 de 64 bits ou posterior.

**Instalação pelo PowerShell:**

```powershell
irm https://get.mangodisk.app | iex
```

**Instalação pelo WinGet (fonte oficial):**

```powershell
winget install --id MangoDisk.MangoDisk --exact --source winget
```

**Instalação manual:** baixe o instalador para Windows na [página oficial](https://mangodisk.app/download) e siga as instruções na tela.

### Linux

**Recomendado:** Ubuntu 22.04 LTS ou posterior, em x64 ou ARM64.

Disponível em pacotes `.deb` e AppImages. A compatibilidade com outras distribuições Linux depende das bibliotecas do sistema e do ambiente gráfico.

**Instalação pelo terminal (Debian/Ubuntu):** este comando detecta a arquitetura e instala o pacote `.deb` correspondente mais recente.

```sh
curl -fsSL https://get.mangodisk.app/linux | bash
```

**Instalação manual:** escolha o pacote da sua arquitetura na [página oficial](https://mangodisk.app/download).

- **Debian/Ubuntu:** instale o pacote `.deb` da sua arquitetura.
- **Outras distribuições:** torne a AppImage executável e execute-a.

## Linha de comando (CLI)

Use o MangoDisk no terminal ou em scripts, com o mesmo mecanismo de limpeza segura do aplicativo para desktop.

### macOS

**Instalação pelo Homebrew:**

```sh
brew install harry0703/tap/mangodisk-cli
```

### Windows

**Instalação pelo PowerShell:**

```powershell
irm https://get.mangodisk.app/cli | iex
```

**Instalação pelo WinGet (fonte oficial):**

```powershell
winget install --id MangoDisk.CLI --exact --source winget
```

### Linux

Ainda não há downloads de uma CLI independente pré-compilada para Linux. Siga as instruções em [Compilar a partir do código-fonte](#compilar-a-partir-do-código-fonte) para compilá-la.

### Exemplos de uso

Se o comando `mangodisk` não estiver disponível logo após a instalação, abra um novo terminal e verifique a instalação:

```sh
mangodisk --version
```

Comandos comuns:

```sh
# Analisar e mostrar itens que podem ser limpos sem alterar nada
mangodisk clean

# Aplicar as mesmas recomendações do aplicativo para desktop
mangodisk clean --apply

# Visualizar todos os itens selecionáveis sem excluir nada
mangodisk clean --apply --selection all --dry-run

# Produzir uma saída JSON legível por máquina
mangodisk clean --format json --no-progress
```

Por padrão, `mangodisk clean` apenas analisa e não modifica arquivos. Para executar a limpeza em um ambiente não interativo, você também precisa informar `--yes` para confirmar a ação. Consulte todas as opções com:

```sh
mangodisk clean --help
```

## Compilar a partir do código-fonte

### Pré-requisitos

- Node.js 24 LTS
- pnpm 11.13.1
- Rust estável

Para dependências específicas de cada plataforma, consulte os [pré-requisitos do Tauri 2](https://v2.tauri.app/start/prerequisites/).

### Obter o código-fonte e executar o aplicativo

```sh
git clone https://github.com/harry0703/MangoDisk.git
cd MangoDisk
pnpm install --frozen-lockfile
pnpm tauri:dev
```

### Executar as verificações obrigatórias

```sh
pnpm check
cargo test --manifest-path src-tauri/Cargo.toml -p mangodisk-core
```

### Gerar o instalador para desktop

```sh
pnpm tauri:build
```

### Compilar a CLI

```sh
pnpm cli:build
```

Compilações locais não incluem a assinatura, a notarização nem os metadados de atualização das versões oficiais do MangoDisk. Use-as apenas para desenvolvimento e validação locais.

## Como contribuir

Relatos de problemas, regras de limpeza, correções e novos recursos são bem-vindos. Leia [`CONTRIBUTING.md`](CONTRIBUTING.md) e [`AGENTS.md`](AGENTS.md) antes de começar.

A cobertura de limpeza rotineira deve usar regras TOML declarativas validadas durante a compilação. Consulte [`src-tauri/crates/mangodisk-core/rules/README.md`](src-tauri/crates/mangodisk-core/rules/README.md) para conhecer o esquema das regras, os limites de segurança e as instruções de validação.

Antes de enviar alterações, execute pelo menos:

```sh
pnpm check
cargo test --manifest-path src-tauri/Cargo.toml -p mangodisk-core
```

Comunique vulnerabilidades de segurança de forma privada pelo GitHub Security Advisories, conforme descrito em [`SECURITY.md`](SECURITY.md). Não abra uma issue pública para relatar uma vulnerabilidade.

## Tecnologias utilizadas

- [Tauri 2](https://tauri.app/): ambiente de execução para desktop e integração com o sistema
- [Rust](https://www.rust-lang.org/): análise, acesso ao sistema de arquivos, validação de segurança e execução da limpeza
- [Vue 3](https://vuejs.org/) e [TypeScript](https://www.typescriptlang.org/): interface do aplicativo para desktop

## Licença

O MangoDisk é um projeto de código aberto sob a [GNU General Public License v3.0](https://github.com/harry0703/MangoDisk/blob/main/LICENSE). Componentes de terceiros continuam sujeitos às respectivas licenças.
