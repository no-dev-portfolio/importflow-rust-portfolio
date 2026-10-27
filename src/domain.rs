use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_CSV_BYTES: usize = 512 * 1024;
pub const MAX_ROWS: usize = 1000;
pub const SAMPLE: &str = "sku,name,quantity,unit_price\nPART-101,制御モジュール,12,4800\nPART-202,精密センサー,24,1250\nPART-315,駆動ユニット,6,8200\n";

#[derive(Clone, Debug, Serialize)]
pub struct Actor {
    pub tenant: String,
    pub role: String,
}
impl Actor {
    pub fn write(&self) -> bool {
        self.role == "manager" || self.role == "operator"
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Submission {
    pub label: String,
    pub csv: String,
    #[serde(default)]
    pub simulate_failures: u8,
}
impl Submission {
    pub fn normalize(mut self) -> Result<Self, Fault> {
        self.label = self.label.trim().to_owned();
        if self.label.is_empty()
            || self.label.chars().count() > 64
            || self.label.chars().any(char::is_control)
        {
            return Err(Fault::Invalid(
                "取込名は制御文字を含まない1〜64文字で指定してください。".into(),
            ));
        }
        if self.csv.is_empty() || self.csv.len() > MAX_CSV_BYTES || self.simulate_failures > 2 {
            return Err(Fault::Invalid(
                "CSVは1〜512KiB、模擬失敗回数は0〜2で指定してください。".into(),
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct Product {
    pub sku: String,
    pub name: String,
    pub quantity: i32,
    pub unit_price: i64,
}
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct Issue {
    pub line: u64,
    pub message: String,
}
#[derive(Debug)]
pub struct ValidatedBatch(Vec<Product>);
impl std::ops::Deref for ValidatedBatch {
    type Target = [Product];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// No partial success: the caller receives either all validated rows or issues.
pub fn validate_csv(input: &str) -> Result<ValidatedBatch, Vec<Issue>> {
    let issue = |line, message: &str| {
        vec![Issue {
            line,
            message: message.into(),
        }]
    };
    if input.len() > MAX_CSV_BYTES {
        return Err(issue(0, "CSVが512KiBを超えています。"));
    }
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    // csv's parser deliberately accepts some malformed quoting. Reject it here.
    if !strict_quotes(input) {
        return Err(issue(
            0,
            "引用符が閉じていないか、CSVの引用符の位置が不正です。",
        ));
    }
    let mut reader = csv::ReaderBuilder::new()
        .flexible(false)
        .from_reader(input.as_bytes());
    let headers = reader
        .headers()
        .map_err(|_| issue(1, "ヘッダーを読めません。"))?;
    if headers.iter().ne(["sku", "name", "quantity", "unit_price"]) {
        return Err(issue(
            1,
            "ヘッダーはsku,name,quantity,unit_priceの順にしてください。",
        ));
    }
    let mut products = Vec::new();
    let mut issues = Vec::new();
    let mut seen = HashSet::new();
    for (i, result) in reader.records().enumerate() {
        let line = i as u64 + 2;
        if i >= MAX_ROWS {
            issues.push(Issue {
                line,
                message: "明細は1000件までです。".into(),
            });
            break;
        }
        let record = match result {
            Ok(r) => r,
            Err(_) => {
                issues.push(Issue {
                    line,
                    message: "列数またはCSV形式が不正です。".into(),
                });
                if issues.len() >= 20 {
                    break;
                }
                continue;
            }
        };
        let sku = &record[0];
        let name = &record[1];
        let quantity = record[2].parse::<i32>();
        let price = record[3].parse::<i64>();
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let valid = (2..=32).contains(&sku.len())
            && sku.as_bytes()[0].is_ascii_alphanumeric()
            && sku
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
            && !name.trim().is_empty()
            && name.chars().count() <= 80
            && !name.chars().any(char::is_control)
            && digits(&record[2])
            && quantity.as_ref().is_ok_and(|n| (1..=100_000).contains(n))
            && digits(&record[3])
            && price.as_ref().is_ok_and(|n| (0..=100_000_000).contains(n));
        if !valid {
            issues.push(Issue {
                line,
                message:
                    "SKU・名称・数量（1〜100000）・単価（0〜100000000の整数）を確認してください。"
                        .into(),
            });
        } else if !seen.insert(sku.to_owned()) {
            issues.push(Issue {
                line,
                message: "同一CSV内でSKUが重複しています。".into(),
            });
        } else {
            products.push(Product {
                sku: sku.into(),
                name: name.into(),
                quantity: quantity.unwrap_or_default(),
                unit_price: price.unwrap_or_default(),
            });
        }
        if issues.len() >= 20 {
            break;
        }
    }
    if products.is_empty() && issues.is_empty() {
        return Err(issue(2, "明細がありません。"));
    }
    products.sort_by(|a, b| a.sku.cmp(&b.sku));
    if issues.is_empty() {
        Ok(ValidatedBatch(products))
    } else {
        Err(issues)
    }
}

fn strict_quotes(input: &str) -> bool {
    enum Field {
        Start,
        Plain,
        Quoted,
        Closed,
    }
    let mut state = Field::Start;
    for b in input.bytes() {
        state = match (state, b) {
            (Field::Start, b'"') => Field::Quoted,
            (Field::Start | Field::Plain | Field::Closed, b',' | b'\r' | b'\n') => Field::Start,
            (Field::Start, _) => Field::Plain,
            (Field::Plain, b'"') => return false,
            (Field::Plain, _) => Field::Plain,
            (Field::Quoted, b'"') => Field::Closed,
            (Field::Quoted, _) => Field::Quoted,
            (Field::Closed, b'"') => Field::Quoted,
            (Field::Closed, _) => return false,
        }
    }
    !matches!(state, Field::Quoted)
}

#[derive(Debug, thiserror::Error)]
pub enum Fault {
    #[error("{0}")]
    Invalid(String),
    #[error("認証が必要です。")]
    Unauthorized,
    #[error("操作権限がありません。")]
    Forbidden,
    #[error("対象がありません。")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    #[error("待機中のジョブが多いため、完了後に再送してください。")]
    Capacity,
    #[error("処理を完了できませんでした。同じ要求キーで再試行してください。")]
    Database(#[from] sqlx::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_and_bom() {
        assert_eq!(validate_csv(SAMPLE).unwrap().len(), 3);
        assert!(validate_csv(&format!("\u{feff}{SAMPLE}")).is_ok());
    }
    #[test]
    fn headers_and_empty() {
        for s in [
            "",
            "sku,name,quantity,unit_price\n",
            "name,sku,quantity,unit_price\n",
        ] {
            assert!(validate_csv(s).is_err());
        }
    }
    #[test]
    fn reject_bad_values() {
        for row in [
            "a,x,1,2",
            "AA,,1,2",
            "AA,x,-1,2",
            "AA,x,1,1.2",
            "AA,x,100001,2",
            "AA,x,1,100000001",
            "AA,x,1e3,2",
            "AA,x,1,2,extra",
            "AA,foo\"bar,1,2",
            "AA,x,1,\"2",
            "AA,x,1,2\nAA,y,1,2",
            "AA,制御\u{0085},1,2",
        ] {
            assert!(
                validate_csv(&format!("sku,name,quantity,unit_price\n{row}")).is_err(),
                "{row}"
            );
        }
    }
    #[test]
    fn quoted_name_and_crlf() {
        assert_eq!(
            validate_csv("sku,name,quantity,unit_price\r\nAA,\"部品,青\",1,0\r\n").unwrap()[0].name,
            "部品,青"
        );
    }
    #[test]
    fn row_and_byte_limits() {
        let text = format!(
            "sku,name,quantity,unit_price\n{}",
            (0..1001)
                .map(|i| format!("A{i},name,1,2\n"))
                .collect::<String>()
        );
        assert!(validate_csv(&text).is_err());
        assert!(validate_csv(&"x".repeat(MAX_CSV_BYTES + 1)).is_err());
    }
    #[test]
    fn submission_boundaries() {
        for label in ["", "\u{0085}", &"a".repeat(65)] {
            assert!(
                Submission {
                    label: label.into(),
                    csv: SAMPLE.into(),
                    simulate_failures: 0
                }
                .normalize()
                .is_err()
            );
        }
    }
    proptest::proptest! { #[test] fn arbitrary_text_does_not_panic(s in ".{0,4096}") { let _=validate_csv(&s); } }
}
