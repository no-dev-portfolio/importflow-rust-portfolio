# ImportFlow v1.0.0 検証報告書

実施日: 2026-10-27（日本時間）
対象: ローカル架空データ用の配布ソース・Dockerアプリ。Codexによる実装者自身の検証であり、独立した第三者監査ではない。

## 結果

| 検査 | 実測結果 |
| --- | --- |
| ツールチェーン | Rust 1.99.1 / Cargo 1.99.1、Docker Linux arm64、PostgreSQL 17 |
| Rust試験 | 単体・プロパティ9件、実DB統合16件、合計25件成功。失敗0・ignored 0 |
| プロパティ試験 | proptestで任意文字列に対する非panicを確認。1つのテストとして数え、生成入力数をテスト件数へ加算しない |
| 繰返し | リース回復・ロック待ち中の期限切れ・試行上限の3件を各3回成功 |
| 整形・静的検査 | cargo fmt --check、cargo clippy --all-targets -- -D warnings成功 |
| フロントエンド | TypeScript型検査・本番ビルド、Nodeテスト3件成功。npm auditは脆弱性0件 |
| Docker | 配布Dockerfileのdemo/verify両ターゲットをビルドし、verifyイメージでDB接続必須の検査を実行 |
| Runtime | Composeで起動、health・readyz・API・OpenAPIが応答。非root uid10001、読み取り専用、health healthy |
| ポート変更 | LISTEN_ADDR=0.0.0.0:9090の実コンテナでhealthコマンド終了コード0 |
| 永続化 | 作成した取込完了ジョブと商品を、Compose down（ボリューム削除なし）→up後に同じID・集計で取得。シード重複なし |
| 終了 | コンテナ停止時にJSONログのshutdown_completeを確認 |
| 画面 | CSV登録→模擬失敗2回→3回目に完了、2行登録・操作履歴を確認。別組織は空、viewerは作成なし |
| モバイル | 390pxでdocument幅390px、ページ全体の横はみ出しなし。表内は横スクロール。ブラウザーerrorログ0件 |

Rust試験は実PostgreSQLの個別スキーマを使う。OSプロセスを大量起動した負荷試験ではなく、複数DB接続とTokioタスクによる競合検査。DBの環境変数がない場合は失敗するため、skipを成功としない。

## 主要な反例

- 同じ要求キーの10件並行送信→1ジョブだけ作成、同じIDを返す。別内容は競合。
- 6並行ワーカー→1回の取得・処理だけ成功。
- 不正行を混ぜたCSV・既存SKUを含むCSV→一部だけ登録しない。
- 共通SKUを持つ2バッチの同時取込→片方だけ全件完了、片方は全件拒否。
- 取消後・リース再取得後の古い結果→商品を追加しない。
- 商品行待ちの間にリース期限を過ぎる→途中挿入もロールバック。
- 3回の一時障害・3回目の期限切れ→failed、管理者の再試行を確認。
- 1,000行・集計10,000,000,000,000,000円→APIは10進文字列を正確に返す。
- 不正メディアタイプ415、本文上限413、複数JSON400、未知フィールド422、組織・役割制御、GraphQLのlimitと組織分離。

## 見つけて修正した問題

CSVライブラリは、引用されていないフィールド内の`"`など、一部の不正な引用符を寛容に受け付けた。`AA,foo"bar,1,2`を含む境界試験が失敗する反例を保存し、引用符の字句検査を追加。閉じていない引用符・不正な引用位置を拒否する元の反例を回帰試験へ追加し、全体検査でも解消を確認した。

修正前の反例は[evidence/review-quotes-before.log](../evidence/review-quotes-before.log)、修正後の全体・反復結果は[evidence/distribution-tests.log](../evidence/distribution-tests.log)を参照。

## 依存監査の結果と例外

**無条件のCargo.lock全件監査には1件の指摘がある。** [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html)はrsa 0.9.10のタイミングサイドチャネルに関する脆弱性で、修正済みバージョンなし。

Cargo.lockにはSQLxの未使用MySQL経路のrsaが含まれる。当作品の選択ビルド（PostgreSQLのみ）について、`cargo tree --locked --edges normal,build --invert rsa`は依存経路なし。`scripts/audit.sh`はこの結果が空であることを検査してから、このIDだけを除外した監査を行う。rsaが選択される変更では先に失敗させる。

全件監査のJSONを[evidence/audit-full.json](../evidence/audit-full.json)に保存。全指摘が0件、または実行シンボル単位の到達解析をしたとは主張しない。別ターゲット・機能変更時には例外を再評価する。

## 証跡と再実行

```sh
docker compose --profile test run --build --rm tests
```

コマンド・ツール版・テスト件数・反復回数は[evidence](../evidence/)に保存。GitHub Actionsの定義は同梱するが、GitHubで実行した結果ではない。Rustの文カバレッジ・大規模負荷・OSコンテナ全体の脆弱性・バックアップ復旧・クラウド実配備は未検証。Claudeによる反証レビューも今回未実施。

画像は実ブラウザー操作時のもの。初期状態と操作後の状態で件数が異なる。表示時刻はブラウザーのタイムゾーンに従う。
