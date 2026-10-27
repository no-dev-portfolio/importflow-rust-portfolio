# Rust開発案件の要求スキル調査

調査日: 2026-10-27（日本時間）。対象: 日本のフリーランス・業務委託案件を中心とする公開募集。

## 方法と結論

Deep Research専用機能はこの環境では利用できないため、通常のWeb検索と募集ページの原文照合を行った。専用機能が生成したレポートではない。掲載タグだけでRust必須と判断せず、必須・尚可・開発環境を分けた。直接本文を取得できなかったものは検索インデックスでの確認と明記する。募集終了例を含み、現在応募できる件数・市場全体の割合・平均単価は算出しない。

Rust案件は一つの技術要件にまとまらない。Webバックエンド、組み込み、データ処理・最適化、ネイティブアプリなどで必要スキルが異なる。今回の作品は**Webバックエンド・非同期データ処理**を対象にする。求人の経験年数、チーム経験、クラウド本番運用を自主制作で満たしたとは扱わない。

## 募集例の照合

| ID | 募集元・案件 | 必須として確認した内容 | 尚可・環境との区別 / 取得範囲 |
| --- | --- | --- | --- |
| R1 | [Adecco / アサイン: ネット証券アプリ](https://freelance.adecco.co.jp/projects/673508/) | Web API、RDB、MQ、Docker、Git等とチーム経験。各5年以上の条件あり | Rust・PostgreSQL・Kafka・GraphQL・AWSは尚可。本文取得済み、2026-10-16更新。検索要約では必須と混ざるため本文で修正 |
| R2 | [テクフリ Rust一覧](https://freelance.techcareer.jp/projects/skills/rust/) 内の配信サービス | Rust 1年以上、バックエンド実装5年以上 | 検索インデックスの募集本文で確認。直接取得は403。経験年数は作品では代替しない |
| R3 | [レバテック: 独自サービス開発](https://freelance.levtech.jp/project/detail/81300/) | 必須欄の全体は取得できず、充足判定に使わない | 検索インデックスでActix-webを使うGraphQL実装、GraphQL・データ基盤の歓迎条件を確認。直接取得403 |
| R4 | [テクフリ Rust一覧](https://freelance.techcareer.jp/projects/skills/rust/) 内のスケジューリング | 最適化・探索・データ構造、Rust/C/C++、遠隔チーム開発 | Actix-web・GCPは環境欄。募集終了の検索本文。Web API作品だけで最適化案件への適合とはしない |
| R5 | [フリーランスHub: 経理SaaS / ROSCA掲載](https://freelance-hub.jp/project/detail/459267/) | Web5年、React/TypeScript2年、RustまたはGo2年 | 会計・LLM・英語は歓迎。[同案件の別掲載](https://freelance.indieverse.co.jp/job_listings/733842)とも照合。重複を2案件として数えない |
| R6 | [ITプロパートナーズ Rust一覧](https://itpropartners.com/index.php/job/engineer/rust) | C/C++またはRustでのOSネイティブ実装、プロセス・メモリ・権限・通信への理解を求める例 | 検索本文のみ。今回のWeb作品では低レイヤの要件は満たさない |

補足: [テックスペース Rust一覧](https://tech-space.jp/language/rust/)の本文にはRust以外が中心の職種も混在した。件数や言語タグをそのままRust必須案件数と解釈していない。[OffersのRust業務委託一覧](https://offers.jp/jobs/skills/274/side-job)は技術タグ・募集概要の補助確認に使い、必須条件の代替には使わない。

## 作品で示す技術と選定理由

| 技術面 | 作品への反映 | 根拠・判断 |
| --- | --- | --- |
| RustによるAPI設計・実装 | Axum / serde / REST / OpenAPI | R1のAPI要件、R2のRust実装。Axum自体を全案件共通の必須とはしない |
| 非同期・並行処理 | Tokio、2ワーカー、CPU検証のspawn_blocking分離 | 同時処理・障害時の扱いを具体的に示す設計上の判断 |
| RDB・SQL | PostgreSQL / SQLx / トランザクション / 一意制約 | R1のRDB要件と、APIデータ整合性 |
| GraphQL | 認証付き読取API、型付きスキーマ、複雑度・深さ制限 | R1/R3の尚可・実装環境への追加対応。変更系はRESTへ集約 |
| Docker・Git・テスト | Compose、ロックファイル、fmt/clippy/test、CI定義 | R1の周辺開発要件。チーム実績とは区別 |
| React/TypeScript | 日本語管理画面、応答競合対策、正確な整数表示 | R5のフルスタック構成に技術面で対応 |
| 安全性・保守性 | unsafe禁止、ValidatedBatch、入力上限、取消・リースの検査 | Rustの型と所有権を業務境界に使う設計上の判断 |

Kafka等の外部MQ、Actix-web、クラウド配備、組み込み、Tauri、最適化アルゴリズムは実装対象外。PostgreSQLのジョブ表を、Kafkaの実装・運用経験や外部MQ経験と同じものにしない。

## 参照した公式技術文書

- [Rust公式](https://rust-lang.org/): ツールチェーン1.99.1を確認しDockerで取得・実測。
- [Axum公式API](https://docs.rs/axum/latest/axum/): Tokio・Tower上のHTTP構成。
- [Tokio公式チュートリアル](https://tokio.rs/tokio/tutorial): 非同期I/OとCPU処理の役割の違い。
- [SQLx公式API](https://docs.rs/sqlx/latest/sqlx/): 非同期SQL。今回の実装は0.8.6を使用、最新版0.9.0への対応を主張しない。
- [async-graphql公式API](https://docs.rs/async-graphql/latest/async_graphql/): 型付きスキーマとクエリ制限。
- [PostgreSQL 17 ロック](https://www.postgresql.org/docs/17/explicit-locking.html): 行ロックと取引。

応募先ごとに必要な技術・実務条件を再確認する。作品のURLは自主制作欄に掲載し、職務経歴・実務経験年数とは分ける。
