# 求人要件と実装の対応

[調査報告](RESEARCH.md)の公開募集をもとにした技術面の対応表。全Rust案件の応募資格・実務経験を満たす認定ではない。

| 求人・評価項目 | 成果物 | 確認の範囲 |
| --- | --- | --- |
| Rustバックエンド | src/domain.rs、api.rs、store.rs、worker.rs | 実装・ローカル検証 |
| 非同期・並行処理 | Tokio、2ワーカー、spawn_blocking、SKIP LOCKED | 実DBの並行取得・古いワーカー排除 |
| 型・所有権・エラー処理 | 非公開内部のValidatedBatch、Result、Fault、unsafe禁止 | 型境界・コンパイル・境界試験 |
| API開発 | REST、OpenAPI、認証付きGraphQL | 実応答・不正入力・組織分離 |
| SQL・RDB設計 | schema.sql、SQLx、取引・一意制約・索引 | 全件取消、重複、DB時計、並行処理 |
| React/TypeScript | 日本語管理画面、世代管理、BigInt金額 | 型検査・Nodeテスト・画面確認 |
| コンテナ | Dockerfile、Compose、非root・read-only | 実ビルド・起動。詳細はVALIDATION |
| 品質・レビュー | fmt、clippy、cargo test、proptest、cargo-audit | ローカル実施記録。独立監査ではない |
| Git/CI | GitHub Actions定義、ロックファイル | 定義を公開。実行結果はGitHub Actions参照 |
| クラウド | CLOUD.mdでAWS構成・追加要件を説明 | 設計説明のみ。AWS配備・IaC実行は未実施 |
| 外部MQ/Kafka | 対象外 | DBキューを外部MQ経験として扱わない |
| Actix-web/Diesel | 対象外 | Axum/SQLxを採用。指定案件は別途対応が必要 |
| Rust実務年数、RDB/MQ等5年以上 | 対応不可 | 実際の職務経歴で判断。自主制作を加算しない |
| チーム開発、英語、金融・会計経験 | 対応不可 | この作品だけで獲得した実績にしない |
| OS・組み込み・FFI・最適化 | 対象外 | その領域の必須条件を満たさない |
