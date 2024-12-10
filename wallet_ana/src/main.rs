use serde_json::Value;
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use clap::{Arg, Command};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("Wallet Analyzer")
        .version("1.0")
        .author("Your Name <you@example.com>")
        .about("Analyzes wallet transactions")
        .arg(Arg::new("address")
            .short('a')
            .long("address")
            .value_name("WALLET_ADDRESS")
            .required(true))
        .get_matches();

    let address = matches.get_one::<String>("address").unwrap();
    let api_key = "9d6f4b41-ce63-4127-9b09-0f702db26740"; // 替换为你的 API 密钥
    let url = format!("https://api.helius.xyz/v0/addresses/{}/transactions?api-key={}", address, api_key);

    let response: Value = reqwest::get(&url).await?.json().await?;

    // 存储与目标地址相关的地址
    let mut related_addresses = HashSet::new();

    // 将结果写入文本文件，若文件存在则续写
    let mut file = OpenOptions::new()
        .write(true)
        .append(true) // 设置为追加模式
        .create(true) // 如果文件不存在则创建
        .open("related_addresses.txt")?; // 打开文件

    // 遍历交易数据
    if let Some(transactions) = response.as_array() {
        for transaction in transactions {
            if let Some(native_transfers) = transaction.get("nativeTransfers").and_then(|v| v.as_array()) {
                for transfer in native_transfers {
                    if let (Some(from), Some(to), Some(amount)) = (
                        transfer.get("fromUserAccount").and_then(|v| v.as_str()),
                        transfer.get("toUserAccount").and_then(|v| v.as_str()),
                        transfer.get("amount").and_then(|v| v.as_i64())
                    ) {
                        if (from == address || to == address) && amount > 2_000_000_0000 {
                            related_addresses.insert(from);
                            related_addresses.insert(to);
                            // 写入地址和金额到文件，金额除以 1_000_000_000
                            writeln!(file, "地址: {}, 金额: {}", from, amount as f64 / 1_000_000_000.0)?;
                            writeln!(file, "地址: {}, 金额: {}", to, amount as f64 / 1_000_000_000.0)?;
                        }
                    }
                }
            }
        }
    }

    // 输出与目标地址相关的所有地址
    println!("相关地址已写入 related_addresses.txt 文件。");

    Ok(())
}


