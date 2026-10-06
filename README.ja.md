<h1 align="center">
  <img src="public/strawberrydisk.png" width="40" alt="StrawberryDisk アプリアイコン"> StrawberryDisk
</h1>

<p align="center"><b>macOS</b>・<b>Windows</b>・<b>Linux</b> 向けのディスククリーンアップ・ストレージ分析・プライバシー保護ツール</p>

<p align="center">
<a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a> · <a href="README.zh-TW.md">繁體中文</a> · 日本語 · <a href="README.ko.md">한국어</a> · <a href="README.ru.md">Русский</a>
</p>

<p align="center">
<a href="https://github.com/Yeake0/StrawberryDisk-updates/releases/latest"><img alt="最新リリース" src="https://img.shields.io/github/v/release/Yeake0/StrawberryDisk-updates?display_name=tag&sort=semver"></a>
  <img alt="macOS 対応" src="https://img.shields.io/badge/macOS-supported-111827?logo=apple&logoColor=white">
  <img alt="Windows 対応" src="https://img.shields.io/badge/Windows-supported-2563eb?logo=windows&logoColor=white">
  <img alt="Linux 対応" src="https://img.shields.io/badge/Linux-supported-f59e0b?logo=linux&logoColor=white">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24c8db?logo=tauri&logoColor=white">
  <img alt="Rust Core" src="https://img.shields.io/badge/core-Rust-b7410e?logo=rust&logoColor=white">
</p>

## StrawberryDisk でできること

> **ストレージ**

### 1. ディープクリーン

システム、アプリ、開発ツール、ローカルプロジェクトに散らばるクリーンアップ対象を一度に見つけます。場所ごとに探す手間を省き、解放できる容量をカテゴリ別にまとめて確認できます。

- **システムとユーザーのキャッシュ**：システムの一時ファイルや診断データ、再作成可能なキャッシュを整理し、空き容量を増やします。
- **アプリキャッシュ**：アプリのキャッシュ、ログ、更新パッケージ、一時データが蓄積してストレージを圧迫するのを防ぎます。
- **ブラウザデータ**：Chrome、Edge、Firefox、Brave、Arc、Opera などのキャッシュや一時的なウェブデータが使用している容量を取り戻します。
- **開発ツールと Xcode**：パッケージマネージャー、IDE、コンパイルキャッシュ、Xcode の開発データが占める大きな容量をすばやく解放します。
- **コンテナキャッシュ**：Docker などの未使用のビルドキャッシュや再作成可能なデータから空き容量を取り戻します。
- **プロジェクトのビルド成果物**：Node.js、Rust、Gradle、Swift、Python、.NET、Godot、CMake などのプロジェクトにある再作成可能な依存関係、キャッシュ、ビルドディレクトリから空き容量を取り戻します。
- **AI モデルとキャッシュ**：容量の大きいローカル AI モデル、ダウンロードキャッシュ、一時転送ファイルをすばやく見つけます。
- **アプリ容量の最適化**：普段の利用に影響を与えずに対応アプリを小さくし、ディスクの空き容量を増やします。

スマート選択なら、安全性に配慮した候補をすばやく選べます。項目ごとの確認や事前の空き容量見積もりもできるため、納得したうえで安心して整理できます。

### 2. 大容量ファイル

容量を多く使う大きなファイルをすばやく見つけ、古いインストーラー、動画、アーカイブなどから空き容量を取り戻せます。フォルダーを一つずつ探す必要はありません。

### 3. 重複ファイル

重複したコピーが占める容量を取り戻しながら、同じ名前でも内容が異なるファイルの誤判定を防ぎます。スマート選択では各グループに少なくとも 1 ファイルを残すため、安心して整理できます。

### 4. ディスク容量分析

ディスク容量が何に使われているかをひと目で把握できます。**ツリーマップ**と**サンバースト**を切り替え、表示する階層の深さを調整して、容量の割合やフォルダー構成を確認できます。ファイル一覧とあわせてフォルダーをたどれば、容量の大きいフォルダーやファイルをすばやく見つけ、何を整理するか判断しやすくなります。

> **プライバシーとセキュリティ**

### 5. プライバシークリーンアップ

閲覧履歴、検索履歴、Cookie、最近使った項目、クリップボードの内容がパソコンに残り続けるのを防ぎます。利用傾向やアクセス履歴、ログイン状態を他人に見られるリスクを抑え、日々のプライバシーを手軽に管理できます。

> **システムツール**

### 6. アプリのアンインストールとクリーンアップ

アプリ本体と関連するキャッシュ、設定、残存ファイルをまとめて削除し、アンインストール後も残る不要なデータから空き容量を取り戻します。個人ファイルの可能性があるデータは慎重に扱い、誤削除のリスクを抑えます。

### 7. スタートアップ項目の管理

不要な起動待ちとバックグラウンド負荷を減らし、パソコンの起動を速くして、動作を軽快にします。必要になったときはいつでも再度有効にできます。

### 8. システム最適化

動作を重くしたり日常の利用を妨げたりする不要な設定を見直します。パフォーマンス、プライバシー、使い方のバランスを整え、より軽快で快適に使える環境にします。

### 9. システムメンテナンス

検索でファイルが見つからない、アイコンが正しく表示されない、音が出ない、ネットワークにつながらないといったよくある問題をすばやく解消します。手作業で原因を探したり複雑なコマンドを使ったりする手間を省き、いつもの状態に戻せます。

> **操作履歴**

### 10. 操作履歴

クリーンアップやシステム設定の変更をあとから確認できます。増えた空き容量、完了した操作、対応が必要な問題をひと目で把握できます。

## リソースの使用状況とメモリ管理

> バージョン 1.1.1 から利用できます

CPU やメモリの使用状況、通信速度、ディスクの動作状況をいつでも確認できます。メモリを多く使っているアプリをすぐに把握でき、空きが少ないときはワンクリックでメモリを解放できます。

メニューバーやタスクバー、システムトレイに表示できるので、メインウィンドウを開かずにパソコンの状態がひと目でわかります。

## AI 解説

> バージョン 1.1.0 から利用できます

項目の用途や、操作すると何が変わるのかがわからないときに役立つ機能です。項目の説明と現在のスキャン結果をもとに、AI が用途や注意点をわかりやすく解説します。あちこち調べる手間を省き、操作が必要かどうかの判断に役立ちます。

ディープクリーンの内蔵ルール、プライバシークリーンアップ、スタートアップ項目の管理、システム最適化、システムメンテナンスに対応し、各項目から解説を確認できます。

公式版では毎日一定回数まで無料で利用でき、お使いの AI サービスにも接続できます。解説はあくまで参考情報であり、実際に操作するかどうかは自分で決められます。

## 安全性とルール

> [!IMPORTANT]
> **StrawberryDisk は、空き容量の確保よりもデータの安全性を優先します。**
> クリーンアップルールとシステム最適化項目は、安全な範囲を明確にし、実際のシステムで検証したものだけを製品版に採用しています。

StrawberryDisk はデフォルトで読み取り専用のスキャンを行います。クリーンアップ、削除、アンインストール、システム設定の変更前に内容を表示し、ユーザーの確認を求めます。操作結果は履歴に保存されます。

システム最適化では、内蔵の検証済み設定だけを使用します。任意のレジストリパス、ターミナルコマンド、スクリプトを実行することはありません。変更後は設定を再度読み取り、影響の大きい項目や管理者権限、再起動が必要な項目を事前にお知らせします。

クリーンアップルールは StrawberryDisk が独自に管理しています。サードパーティ製プロジェクトは調査の手がかりとしてのみ参照し、信頼できる情報源、安全な範囲、実際のシステムでの動作を確認してから採用します。安全性を明確に確認できない内容はルールに含めません。

ルールライブラリと変更履歴はすべて公開されています：[StrawberryDisk のクリーンアップルールを見る](https://github.com/Yeake0/StrawberryDisk/tree/main/src-tauri/crates/strawberrydisk-core/rules)。

## ご利用前の注意事項

> [!CAUTION]
>
> 1. クリーンアップ、完全削除、アンインストールは元に戻せない場合があります。実行前に内容を確認し、重要なデータは必ずバックアップしてください。
> 2. システムメンテナンスの実行や、スタートアップ項目・システム設定の変更前に、対象項目の用途と影響を確認してください。
> 3. システム最適化の一部は、セキュリティ、プライバシー、バッテリー駆動時間、システムアップデートの動作に影響する場合があります。

## デスクトップ版

[公式ダウンロードページ](https://github.com/Yeake0/StrawberryDisk-updates/releases/latest) または [GitHub Releases](https://github.com/Yeake0/StrawberryDisk-updates/releases/latest) から、お使いの OS に合ったファイルをダウンロードし、以下の手順でインストールしてください。

### Windows

**動作環境：** 64 ビット版 Windows 10 以降。

**手動でインストール：** [公式ダウンロードページ](https://github.com/Yeake0/StrawberryDisk-updates/releases/latest) から Windows 用インストーラーをダウンロードし、画面の案内に従ってインストールします。

## コマンドライン版（CLI）

### 使用例

インストール後に `strawberrydisk` コマンドが見つからない場合は、新しいターミナルを開いてからバージョンを確認してください。

```sh
strawberrydisk --version
```

よく使うコマンド：

```sh
# 変更を加えず、削除可能な内容をスキャンして表示
strawberrydisk clean

# デスクトップ版と同じスマート選択を適用
strawberrydisk clean --apply

# ファイルを削除せず、選択可能な内容をすべてプレビュー
strawberrydisk clean --apply --selection all --dry-run

# 機械処理しやすい JSON 形式で出力
strawberrydisk clean --format json --no-progress
```

`strawberrydisk clean` は既定でスキャンのみを行い、ファイルを変更しません。非対話環境で実際にクリーンアップする場合は、明示的な確認として `--yes` も指定する必要があります。利用できるすべてのオプションは次のコマンドで確認できます。

```sh
strawberrydisk clean --help
```

## ソースからビルド

### 前提条件

- Node.js 24 LTS
- pnpm 11.13.1
- 安定版 Rust

プラットフォームごとの依存パッケージについては、[Tauri 2 の前提条件](https://v2.tauri.app/start/prerequisites/) を参照してください。

### ソースを取得してデスクトップアプリを実行

```sh
git clone https://github.com/Yeake0/StrawberryDisk.git
cd StrawberryDisk
pnpm install --frozen-lockfile
pnpm tauri:dev
```

### 必要なチェックを実行

```sh
pnpm check
cargo test --manifest-path src-tauri/Cargo.toml -p strawberrydisk-core
```

### デスクトップインストーラーをビルド

```sh
pnpm tauri:build
```

### CLI をビルド

```sh
pnpm cli:build
```

ローカルビルドには、StrawberryDisk の公式リリースで提供される署名、公証、アップデート用メタデータは含まれません。開発と検証にのみ使用してください。

## 貢献

不具合報告、クリーンアップルール、修正、新機能の提案を歓迎します。作業を始める前に [`CONTRIBUTING.md`](CONTRIBUTING.md) と [`AGENTS.md`](AGENTS.md) をお読みください。

通常のクリーンアップ対象は、ビルド時に検証される宣言的な TOML ルールとして追加してください。ルールスキーマ、セーフティ制約、検証手順については [`src-tauri/crates/strawberrydisk-core/rules/README.md`](src-tauri/crates/strawberrydisk-core/rules/README.md) を参照してください。

変更を提出する前に、少なくとも次を実行してください:

```sh
pnpm check
cargo test --manifest-path src-tauri/Cargo.toml -p strawberrydisk-core
```

セキュリティ上の問題は、[`SECURITY.md`](SECURITY.md) の案内に従って GitHub Security Advisories から非公開で報告してください。公開 Issue には投稿しないでください。

## 技術スタック

- [Tauri 2](https://tauri.app/): デスクトップランタイムおよびシステム統合
- [Rust](https://www.rust-lang.org/): スキャン、ファイルシステムアクセス、安全性の検証、クリーンアップ実行
- [Vue 3](https://vuejs.org/) および [TypeScript](https://www.typescriptlang.org/): デスクトップユーザーインターフェース

## ライセンス

StrawberryDisk は [GNU General Public License v3.0](https://github.com/Yeake0/StrawberryDisk/blob/main/LICENSE) に基づくオープンソースソフトウェアです。サードパーティ製コンポーネントには、それぞれのライセンスが適用されます。
