# 応募時の紹介例

## 自主制作: ImportFlow

Rust / Axum / Tokio / SQLx / PostgreSQLで、製造部品CSVの非同期取込管理アプリを制作しました。React/TypeScriptの画面から受付・検証結果・操作履歴を確認できます。

型による検証済みデータの境界、DB取引による全件成功・全件拒否、同じ要求の二重登録防止、複数ワーカーの取得分離、取消・処理期限後の古い結果の排除を実装しています。RESTに加え、組織別に認証された読取専用GraphQLを備えています。

実PostgreSQLによる並行処理・ロールバックの試験、プロパティテスト、fmt/clippy、依存監査、Docker起動の記録を同梱しています。コードの要点と選定理由は設計書から確認できます。

架空データのローカル作品です。Rust実務年数、クラウド運用、外部MQの運用、受託納品実績とは区分して提示します。業務経歴URLと自主制作URLは分けて記載してください。自主制作URL: https://github.com/no-dev-portfolio/importflow-rust-portfolio
